//! Native thumbnail capability, scheduling and contextual result delivery.

pub mod cache;
pub mod provider;
pub mod request;
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
use std::sync::{Arc, Condvar, Mutex};
#[cfg(not(feature = "test-utils"))]
use std::time::Duration;
use std::time::Instant;

use cache::ThumbnailCache;
use provider::ThumbnailProvider;
use scheduler::{ResultPublisher, ThumbnailScheduler, ThumbnailSubscriber};
use types::{ThumbnailCacheKey, ThumbnailCandidate, ThumbnailState};

use crate::ipc::types::{ThumbnailPriority, ThumbnailResultEvent, ThumbnailResultKind};
use crate::resource_coordinator::ResourceCoordinator;

pub const MAX_RESULTS_PER_BATCH: usize = 8;
pub const MAX_RESULT_BYTES_PER_BATCH: usize = 512 * 1024;
pub const PARTIAL_BATCH_FLUSH_MILLIS: u64 = 16;

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
    batch_wake: Arc<(Mutex<bool>, Condvar)>,
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
        let batch_wake = Arc::new((Mutex::new(false), Condvar::new()));
        let scheduler = ThumbnailScheduler::new_with_progress_and_metadata_verifier(
            provider,
            coordinator,
            result_publisher(&cache, &batches, &emitter, &clock, &batch_wake),
            result_publisher(&cache, &batches, &emitter, &clock, &batch_wake),
            Arc::new(|candidate| candidate.matches_current_metadata()),
        );
        let service = Self {
            cache,
            scheduler,
            emitter,
            batches,
            clock,
            batch_wake,
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
        let batch_timer = BatchTimer {
            batches: Arc::clone(&self.batches),
            emitter: Arc::clone(&self.emitter),
            clock: Arc::clone(&self.clock),
            stop: Arc::clone(&self.batch_timer_stop),
            wake: Arc::clone(&self.batch_wake),
        };
        let timer = std::thread::spawn(move || batch_timer.run());
        *self.batch_timer.lock().expect("thumbnail batch timer lock") = Some(timer);
    }

    pub fn set_emitter(&self, emitter: ResultEmitter) {
        *self.emitter.lock().expect("thumbnail emitter lock") = Some(emitter);
    }

    pub fn request(&self, subscribers: Vec<(ThumbnailSubscriber, ThumbnailCandidate)>) {
        let now = now_seconds();
        let file_requests = subscribers
            .into_iter()
            .filter(|(_, candidate)| !candidate.is_directory);
        for (subscriber, candidate) in file_requests {
            self.publish_cached_or_submit(subscriber, candidate, now);
        }
    }

    fn publish_cached_or_submit(
        &self,
        subscriber: ThumbnailSubscriber,
        candidate: ThumbnailCandidate,
        now: u64,
    ) {
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        let cached = self
            .cache
            .lock()
            .expect("thumbnail cache lock")
            .get(&key, now);
        let Some(state) = cached else {
            self.scheduler.submit(subscriber, candidate);
            return;
        };
        enqueue_batch(
            &self.batches,
            &self.emitter,
            vec![result_event(subscriber, candidate, state)],
            (self.clock)(),
            &self.batch_wake,
        );
    }

    pub fn replace_scope(
        &self,
        subscriber: ThumbnailSubscriber,
        revision: u64,
        candidates: Vec<(ThumbnailCandidate, ThumbnailPriority, u32)>,
    ) -> usize {
        let requested = candidates.len();
        let now = now_seconds();
        let file_candidates = candidates
            .into_iter()
            .filter(|(candidate, _, _)| !candidate.is_directory);
        let mut pending = Vec::new();
        for desired in file_candidates {
            self.publish_cached_or_queue(&subscriber, desired, now, &mut pending);
        }
        let pending_count = pending.len();
        let accepted_pending = self.scheduler.replace_scope(subscriber, revision, pending);
        requested - pending_count + accepted_pending
    }

    fn publish_cached_or_queue(
        &self,
        subscriber: &ThumbnailSubscriber,
        desired: (ThumbnailCandidate, ThumbnailPriority, u32),
        now: u64,
        pending: &mut Vec<(ThumbnailCandidate, ThumbnailPriority, u32)>,
    ) {
        let needs_generation = self.publish_if_cached(subscriber, &desired.0, now);
        if needs_generation {
            pending.push(desired);
        }
    }

    fn publish_if_cached(
        &self,
        subscriber: &ThumbnailSubscriber,
        candidate: &ThumbnailCandidate,
        now: u64,
    ) -> bool {
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        let cached = self
            .cache
            .lock()
            .expect("thumbnail cache lock")
            .get_with_upgrade(&key, now);
        let Some((state, should_upgrade)) = cached else {
            return true;
        };
        self.publish_cached(subscriber, candidate, state);
        should_upgrade
    }

    fn publish_cached(
        &self,
        subscriber: &ThumbnailSubscriber,
        candidate: &ThumbnailCandidate,
        state: ThumbnailState,
    ) {
        enqueue_batch(
            &self.batches,
            &self.emitter,
            vec![result_event(subscriber.clone(), candidate.clone(), state)],
            (self.clock)(),
            &self.batch_wake,
        );
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
        self.stop_batch_timer();
    }

    #[cfg(not(feature = "test-utils"))]
    fn stop_batch_timer(&self) {
        self.batch_timer_stop.store(true, Ordering::Release);
        self.batch_wake.1.notify_all();
        let timer = self
            .batch_timer
            .lock()
            .expect("thumbnail batch timer lock")
            .take();
        if let Some(timer) = timer {
            let _ = timer.join();
        }
    }
}

#[cfg(not(feature = "test-utils"))]
struct BatchTimer {
    batches: Arc<Mutex<BatchState>>,
    emitter: Arc<Mutex<Option<ResultEmitter>>>,
    clock: ThumbnailClock,
    stop: Arc<AtomicBool>,
    wake: Arc<(Mutex<bool>, Condvar)>,
}

#[cfg(not(feature = "test-utils"))]
impl BatchTimer {
    fn run(self) {
        while self.wait_for_signal() {
            std::thread::sleep(Duration::from_millis(PARTIAL_BATCH_FLUSH_MILLIS));
            flush_due_batch(&self.batches, &self.emitter, (self.clock)());
        }
    }

    fn wait_for_signal(&self) -> bool {
        let (lock, ready) = &*self.wake;
        let mut signalled = lock.lock().expect("thumbnail batch wake lock");
        while !*signalled && !self.stop.load(Ordering::Acquire) {
            signalled = ready.wait(signalled).expect("thumbnail batch wake");
        }
        if self.stop.load(Ordering::Acquire) {
            return false;
        }
        *signalled = false;
        true
    }
}

fn result_publisher(
    cache: &Arc<Mutex<ThumbnailCache>>,
    batches: &Arc<Mutex<BatchState>>,
    emitter: &Arc<Mutex<Option<ResultEmitter>>>,
    clock: &ThumbnailClock,
    batch_wake: &Arc<(Mutex<bool>, Condvar)>,
) -> ResultPublisher {
    let cache = Arc::clone(cache);
    let batches = Arc::clone(batches);
    let emitter = Arc::clone(emitter);
    let clock = Arc::clone(clock);
    let batch_wake = Arc::clone(batch_wake);
    Arc::new(move |candidate, state, subscribers| {
        let changed = cache.lock().expect("thumbnail cache lock").insert(
            ThumbnailCacheKey(candidate.fingerprint.clone()),
            state.clone(),
            now_seconds(),
        );
        if !changed {
            return;
        }
        let events = subscribers
            .into_iter()
            .map(|subscriber| result_event(subscriber, candidate.clone(), state.clone()))
            .collect();
        if matches!(
            state,
            ThumbnailState::Ready {
                quality: crate::ipc::types::ThumbnailQuality::Low,
                ..
            }
        ) {
            emit_batches(&emitter, split_result_events(events));
        } else {
            enqueue_batch(&batches, &emitter, events, clock(), &batch_wake);
        }
    })
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
    batch_wake: &Arc<(Mutex<bool>, Condvar)>,
) {
    let ready_batches = {
        let mut state = batches.lock().expect("thumbnail batches lock");
        state.events.extend(events);
        let mut ready = Vec::new();
        while exceeds_one_batch(&state.events) {
            let count = full_batch_len(&state.events);
            state.last_flush_millis = now_millis;
            ready.push(state.events.drain(..count).collect());
        }
        ready
    };
    emit_batches(emitter, ready_batches);
    if !batches
        .lock()
        .expect("thumbnail batches lock")
        .events
        .is_empty()
    {
        *batch_wake.0.lock().expect("thumbnail batch wake lock") = true;
        batch_wake.1.notify_one();
    }
}

fn result_event_weight(event: &ThumbnailResultEvent) -> usize {
    event.data_url.as_ref().map_or(1, String::len)
}

fn split_result_events(events: Vec<ThumbnailResultEvent>) -> Vec<Vec<ThumbnailResultEvent>> {
    let mut ready = Vec::new();
    let mut batch = Vec::new();
    let mut batch_bytes = 0;
    for event in events {
        let event_bytes = result_event_weight(&event);
        if !batch.is_empty()
            && (batch.len() >= MAX_RESULTS_PER_BATCH
                || batch_bytes + event_bytes > MAX_RESULT_BYTES_PER_BATCH)
        {
            ready.push(std::mem::take(&mut batch));
            batch_bytes = 0;
        }
        batch_bytes += event_bytes;
        batch.push(event);
    }
    if !batch.is_empty() {
        ready.push(batch);
    }
    ready
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
    crate::epoch::now_since_epoch().as_secs()
}

fn result_event(
    subscriber: ThumbnailSubscriber,
    candidate: ThumbnailCandidate,
    state: ThumbnailState,
) -> ThumbnailResultEvent {
    let (state, quality, data_url) = match state {
        ThumbnailState::Ready { data_url, quality } => (
            ThumbnailResultKind::Ready,
            Some(quality),
            Some(data_url.as_str().to_owned()),
        ),
        ThumbnailState::Unavailable => (ThumbnailResultKind::Unavailable, None, None),
        ThumbnailState::Failed => (ThumbnailResultKind::Failed, None, None),
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
        quality,
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

fn exceeds_one_batch(events: &[ThumbnailResultEvent]) -> bool {
    let total_bytes = events.iter().map(result_event_weight).sum::<usize>();
    events.len() >= MAX_RESULTS_PER_BATCH || total_bytes > MAX_RESULT_BYTES_PER_BATCH
}

/// How many leading events fit one batch; always at least one.
fn full_batch_len(events: &[ThumbnailResultEvent]) -> usize {
    let mut batch_bytes = 0;
    let mut count = 0;
    for event in events {
        let event_bytes = result_event_weight(event);
        let batch_full = count >= MAX_RESULTS_PER_BATCH
            || batch_bytes + event_bytes > MAX_RESULT_BYTES_PER_BATCH;
        if count > 0 && batch_full {
            break;
        }
        count += 1;
        batch_bytes += event_bytes;
    }
    count
}
