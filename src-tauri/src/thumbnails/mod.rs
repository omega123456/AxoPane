//! Native thumbnail capability, scheduling and contextual result delivery.

pub mod cache;
pub mod provider;
pub mod scheduler;
pub mod types;

#[cfg(feature = "test-utils")]
mod fake_provider;
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
mod macos;
#[cfg(all(not(feature = "test-utils"), not(any(target_os = "macos", windows))))]
mod unsupported;
#[cfg(all(not(feature = "test-utils"), windows))]
mod windows;

#[cfg(not(feature = "test-utils"))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(not(feature = "test-utils"))]
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cache::ThumbnailCache;
use provider::ThumbnailProvider;
use scheduler::{ThumbnailScheduler, ThumbnailSubscriber};
use types::{ThumbnailCacheKey, ThumbnailCandidate, ThumbnailState};

use crate::ipc::types::{ThumbnailResultEvent, ThumbnailResultKind};
use crate::resource_coordinator::ResourceCoordinator;

pub const MAX_RESULTS_PER_BATCH: usize = 8;
pub const PARTIAL_BATCH_FLUSH_MILLIS: u64 = 50;

type ResultEmitter = Arc<dyn Fn(Vec<ThumbnailResultEvent>) + Send + Sync>;
pub type ThumbnailClock = Arc<dyn Fn() -> u64 + Send + Sync>;

struct BatchState {
    events: Vec<ThumbnailResultEvent>,
    last_flush_millis: u64,
}

/// Application-owned thumbnail service. The scheduler never knows about
/// Tauri; this boundary owns cache admission and result batching.
pub struct ThumbnailService {
    cache: Arc<Mutex<ThumbnailCache>>,
    scheduler: ThumbnailScheduler,
    emitter: Arc<Mutex<Option<ResultEmitter>>>,
    batches: Arc<Mutex<BatchState>>,
    clock: ThumbnailClock,
    #[cfg(not(feature = "test-utils"))]
    batch_timer_stop: Arc<AtomicBool>,
    #[cfg(not(feature = "test-utils"))]
    batch_timer: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl ThumbnailService {
    pub fn new(coordinator: Arc<ResourceCoordinator>) -> Self {
        let started = Instant::now();
        Self::new_with_parts(
            platform_provider().into(),
            coordinator,
            Arc::new(move || started.elapsed().as_millis() as u64),
        )
    }

    fn new_with_parts(
        provider: Arc<dyn ThumbnailProvider>,
        coordinator: Arc<ResourceCoordinator>,
        clock: ThumbnailClock,
    ) -> Self {
        let cache = Arc::new(Mutex::new(ThumbnailCache::new()));
        let emitter: Arc<Mutex<Option<ResultEmitter>>> = Arc::new(Mutex::new(None));
        let batches = Arc::new(Mutex::new(BatchState {
            events: Vec::new(),
            last_flush_millis: clock(),
        }));
        let cache_for_completion = Arc::clone(&cache);
        let emitter_for_completion = Arc::clone(&emitter);
        let batches_for_completion = Arc::clone(&batches);
        let clock_for_completion = Arc::clone(&clock);
        let scheduler = ThumbnailScheduler::new_with_metadata_verifier(
            provider,
            coordinator,
            Arc::new(move |candidate, state, subscribers| {
                cache_for_completion
                    .lock()
                    .expect("thumbnail cache lock")
                    .insert(
                        ThumbnailCacheKey(candidate.fingerprint.clone()),
                        state.clone(),
                        now_seconds(),
                    );
                let events = subscribers
                    .into_iter()
                    .map(|subscriber| result_event(subscriber, candidate.clone(), state.clone()))
                    .collect();
                enqueue_batch(
                    &batches_for_completion,
                    &emitter_for_completion,
                    events,
                    clock_for_completion(),
                );
            }),
            Arc::new(|candidate| candidate.matches_current_metadata()),
        );
        let service = Self {
            cache,
            scheduler,
            emitter,
            batches,
            clock,
            #[cfg(not(feature = "test-utils"))]
            batch_timer_stop: Arc::new(AtomicBool::new(false)),
            #[cfg(not(feature = "test-utils"))]
            batch_timer: Mutex::new(None),
        };
        #[cfg(not(feature = "test-utils"))]
        service.start_batch_timer();
        service
    }

    #[cfg(feature = "test-utils")]
    pub fn new_with_provider_and_clock(
        provider: Arc<dyn ThumbnailProvider>,
        coordinator: Arc<ResourceCoordinator>,
        clock: ThumbnailClock,
    ) -> Self {
        Self::new_with_parts(provider, coordinator, clock)
    }

    #[cfg(not(feature = "test-utils"))]
    fn start_batch_timer(&self) {
        let batches = Arc::clone(&self.batches);
        let emitter = Arc::clone(&self.emitter);
        let clock = Arc::clone(&self.clock);
        let stop = Arc::clone(&self.batch_timer_stop);
        let timer = std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(PARTIAL_BATCH_FLUSH_MILLIS));
                flush_due_batch(&batches, &emitter, clock());
            }
        });
        *self.batch_timer.lock().expect("thumbnail batch timer lock") = Some(timer);
    }

    pub fn set_emitter(&self, emitter: ResultEmitter) {
        *self.emitter.lock().expect("thumbnail emitter lock") = Some(emitter);
    }

    pub fn request(&self, subscribers: Vec<(ThumbnailSubscriber, ThumbnailCandidate)>) {
        let now = now_seconds();
        for (subscriber, candidate) in subscribers {
            // Cache admission needs the same re-stat as native admission: a
            // stale listing identity must neither emit nor populate a cache.
            if candidate.is_directory || !candidate.matches_current_metadata() {
                continue;
            }
            let key = ThumbnailCacheKey(candidate.fingerprint.clone());
            if let Some(state) = self
                .cache
                .lock()
                .expect("thumbnail cache lock")
                .get(&key, now)
            {
                enqueue_batch(
                    &self.batches,
                    &self.emitter,
                    vec![result_event(subscriber, candidate, state)],
                    (self.clock)(),
                );
            } else {
                self.scheduler.submit(subscriber, candidate);
            }
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn flush_due(&self) {
        flush_due_batch(&self.batches, &self.emitter, (self.clock)());
    }

    #[cfg(feature = "test-utils")]
    pub fn cache_len(&self) -> usize {
        self.cache.lock().expect("thumbnail cache lock").len()
    }

    pub fn cancel(&self, pane_id: &str, tab_id: &str, path: &str, generation: u64) {
        self.scheduler
            .cancel_scope(pane_id, tab_id, path, generation);
    }

    pub fn shutdown(&self) {
        self.scheduler.shutdown();
        #[cfg(not(feature = "test-utils"))]
        {
            self.batch_timer_stop.store(true, Ordering::Release);
            if let Some(timer) = self
                .batch_timer
                .lock()
                .expect("thumbnail batch timer lock")
                .take()
            {
                let _ = timer.join();
            }
        }
    }
}

impl Drop for ThumbnailService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn enqueue_batch(
    batches: &Arc<Mutex<BatchState>>,
    emitter: &Arc<Mutex<Option<ResultEmitter>>>,
    events: Vec<ThumbnailResultEvent>,
    now_millis: u64,
) {
    let ready_batches = {
        let mut state = batches.lock().expect("thumbnail batches lock");
        state.events.extend(events);
        let mut ready = Vec::new();
        while state.events.len() >= MAX_RESULTS_PER_BATCH {
            state.last_flush_millis = now_millis;
            ready.push(state.events.drain(..MAX_RESULTS_PER_BATCH).collect());
        }
        ready
    };
    emit_batches(emitter, ready_batches);
}

fn flush_due_batch(
    batches: &Arc<Mutex<BatchState>>,
    emitter: &Arc<Mutex<Option<ResultEmitter>>>,
    now_millis: u64,
) {
    let batch = {
        let mut state = batches.lock().expect("thumbnail batches lock");
        if state.events.is_empty()
            || now_millis.saturating_sub(state.last_flush_millis) < PARTIAL_BATCH_FLUSH_MILLIS
        {
            None
        } else {
            state.last_flush_millis = now_millis;
            Some(std::mem::take(&mut state.events))
        }
    };
    emit_batches(emitter, batch.into_iter().collect());
}

fn emit_batches(
    emitter: &Arc<Mutex<Option<ResultEmitter>>>,
    batches: Vec<Vec<ThumbnailResultEvent>>,
) {
    if let Some(emit) = emitter.lock().expect("thumbnail emitter lock").clone() {
        for batch in batches {
            emit(batch);
        }
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn result_event(
    subscriber: ThumbnailSubscriber,
    candidate: ThumbnailCandidate,
    state: ThumbnailState,
) -> ThumbnailResultEvent {
    let (state, data_url) = match state {
        ThumbnailState::Ready { data_url } => (ThumbnailResultKind::Ready, Some(data_url)),
        ThumbnailState::Unavailable => (ThumbnailResultKind::Unavailable, None),
        ThumbnailState::Failed => (ThumbnailResultKind::Failed, None),
    };
    ThumbnailResultEvent {
        pane_id: subscriber.pane_id,
        tab_id: subscriber.tab_id,
        path: subscriber.path,
        generation: subscriber.generation,
        fingerprint_path: candidate.fingerprint.path.to_string_lossy().into_owned(),
        modified_unix_seconds: candidate.fingerprint.modified_unix_seconds,
        size_bytes: candidate.fingerprint.size_bytes,
        state,
        data_url,
    }
}

pub fn platform_provider() -> Box<dyn ThumbnailProvider> {
    #[cfg(feature = "test-utils")]
    {
        Box::new(fake_provider::FakeThumbnailProvider)
    }
    #[cfg(all(not(feature = "test-utils"), windows))]
    {
        Box::new(windows::WindowsThumbnailProvider::default())
    }
    #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
    {
        Box::new(macos::MacosThumbnailProvider::default())
    }
    #[cfg(all(not(feature = "test-utils"), not(any(windows, target_os = "macos"))))]
    {
        Box::new(unsupported::UnsupportedThumbnailProvider)
    }
}
