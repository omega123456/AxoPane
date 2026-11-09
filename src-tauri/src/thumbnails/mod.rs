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

use cache::{ThumbnailCache, ThumbnailCacheEntry};
use provider::ThumbnailProvider;
use scheduler::{
    DesiredThumbnail, ResultPublisher, SchedulerParts, ScopeRequest, ThumbnailScheduler,
    ThumbnailSubscriber,
};
use types::{ThumbnailCacheKey, ThumbnailCandidate, ThumbnailState};

use crate::ipc::types::{ThumbnailResultEvent, ThumbnailResultKind};
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

impl BatchState {
    fn take_full_batches(&mut self, now_millis: u64) -> Vec<Vec<ThumbnailResultEvent>> {
        let mut ready = Vec::new();
        while exceeds_one_batch(&self.events) {
            let count = full_batch_len(&self.events);
            self.last_flush_millis = now_millis;
            ready.push(self.events.drain(..count).collect());
        }
        ready
    }

    fn take_if_due(&mut self, now_millis: u64) -> Option<Vec<ThumbnailResultEvent>> {
        let waited_millis = now_millis.saturating_sub(self.last_flush_millis);
        let not_due = self.events.is_empty() || waited_millis < PARTIAL_BATCH_FLUSH_MILLIS;
        if not_due {
            return None;
        }
        self.last_flush_millis = now_millis;
        Some(std::mem::take(&mut self.events))
    }
}

type BatchWake = Arc<(Mutex<bool>, Condvar)>;

#[derive(Clone)]
struct BatchSink {
    batches: Arc<Mutex<BatchState>>,
    emitter: Arc<Mutex<Option<ResultEmitter>>>,
    wake: BatchWake,
}

struct VisibleRequest {
    subscriber: ThumbnailSubscriber,
    candidate: ThumbnailCandidate,
    cached: Option<ThumbnailState>,
}

#[derive(Clone, Copy)]
struct CacheVisit<'visit> {
    subscriber: &'visit ThumbnailSubscriber,
    now_seconds: u64,
}

pub struct ThumbnailService {
    cache: Arc<Mutex<ThumbnailCache>>,
    scheduler: ThumbnailScheduler,
    sink: BatchSink,
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
        let sink = BatchSink {
            batches: Arc::new(Mutex::new(BatchState {
                events: Vec::new(),
                last_flush_millis: clock(),
            })),
            emitter: Arc::new(Mutex::new(None)),
            wake: Arc::new((Mutex::new(false), Condvar::new())),
        };
        let publisher = ResultPublisherParts {
            cache: Arc::clone(&cache),
            sink: sink.clone(),
            clock: Arc::clone(&clock),
        };
        let parts = SchedulerParts::new(provider, coordinator, publisher.clone().into_publisher())
            .with_progress(publisher.into_publisher())
            .with_metadata_matcher(Arc::new(|candidate| candidate.matches_current_metadata()));
        let scheduler = ThumbnailScheduler::from_parts(parts);
        let service = Self {
            cache,
            scheduler,
            sink,
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
        let batch_timer = BatchTimer {
            sink: self.sink.clone(),
            clock: Arc::clone(&self.clock),
            stop: Arc::clone(&self.batch_timer_stop),
        };
        let timer = std::thread::spawn(move || batch_timer.run());
        *self.batch_timer.lock().expect("thumbnail batch timer lock") = Some(timer);
    }

    pub fn set_emitter(&self, emitter: ResultEmitter) {
        *self.sink.emitter.lock().expect("thumbnail emitter lock") = Some(emitter);
    }

    pub fn request(&self, subscribers: Vec<(ThumbnailSubscriber, ThumbnailCandidate)>) {
        let now = now_seconds();
        let file_requests = subscribers
            .into_iter()
            .filter(|(_, candidate)| !candidate.is_directory);
        for (subscriber, candidate) in file_requests {
            let cached = self.cached_state(&candidate, now);
            self.publish_or_submit(VisibleRequest {
                subscriber,
                candidate,
                cached,
            });
        }
    }

    fn cached_state(&self, candidate: &ThumbnailCandidate, now: u64) -> Option<ThumbnailState> {
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        self.cache
            .lock()
            .expect("thumbnail cache lock")
            .get(&key, now)
    }

    fn publish_or_submit(&self, request: VisibleRequest) {
        let VisibleRequest {
            subscriber,
            candidate,
            cached,
        } = request;
        let Some(state) = cached else {
            self.scheduler.submit(subscriber, candidate);
            return;
        };
        self.publish_event(result_event(subscriber, candidate, state));
    }

    pub fn replace_scope(&self, request: ScopeRequest) -> usize {
        let ScopeRequest {
            subscriber,
            revision,
            candidates,
        } = request;
        let requested = candidates.len();
        let visit = CacheVisit {
            subscriber: &subscriber,
            now_seconds: now_seconds(),
        };
        let pending: Vec<DesiredThumbnail> = candidates
            .into_iter()
            .filter(|desired| !desired.candidate.is_directory)
            .filter(|desired| self.publish_if_cached(visit, &desired.candidate))
            .collect();
        let pending_count = pending.len();
        let accepted_pending = self.scheduler.replace_scope(ScopeRequest {
            subscriber,
            revision,
            candidates: pending,
        });
        requested - pending_count + accepted_pending
    }

    fn publish_if_cached(&self, visit: CacheVisit<'_>, candidate: &ThumbnailCandidate) -> bool {
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        let cached = self
            .cache
            .lock()
            .expect("thumbnail cache lock")
            .get_with_upgrade(&key, visit.now_seconds);
        let Some(lookup) = cached else {
            return true;
        };
        self.publish_event(result_event(
            visit.subscriber.clone(),
            candidate.clone(),
            lookup.state,
        ));
        lookup.should_upgrade
    }

    fn publish_event(&self, event: ThumbnailResultEvent) {
        self.sink.enqueue(vec![event], (self.clock)());
    }

    #[cfg(feature = "test-utils")]
    pub fn flush_due(&self) {
        self.sink.flush_due((self.clock)());
    }

    #[cfg(feature = "test-utils")]
    pub fn cache_len(&self) -> usize {
        self.cache.lock().expect("thumbnail cache lock").len()
    }

    pub fn cancel(&self, scope: &ThumbnailSubscriber) {
        self.scheduler.cancel_scope(scope);
    }

    pub fn shutdown(&self) {
        self.scheduler.shutdown();
        #[cfg(not(feature = "test-utils"))]
        self.stop_batch_timer();
    }

    #[cfg(not(feature = "test-utils"))]
    fn stop_batch_timer(&self) {
        self.batch_timer_stop.store(true, Ordering::Release);
        self.sink.wake.1.notify_all();
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
    sink: BatchSink,
    clock: ThumbnailClock,
    stop: Arc<AtomicBool>,
}

#[cfg(not(feature = "test-utils"))]
impl BatchTimer {
    fn run(self) {
        while self.wait_for_signal() {
            std::thread::sleep(Duration::from_millis(PARTIAL_BATCH_FLUSH_MILLIS));
            self.sink.flush_due((self.clock)());
        }
    }

    fn should_keep_waiting(&self, signalled: bool) -> bool {
        let stopping = self.stop.load(Ordering::Acquire);
        !signalled && !stopping
    }

    fn wait_for_signal(&self) -> bool {
        let (lock, ready) = &*self.sink.wake;
        let mut signalled = lock.lock().expect("thumbnail batch wake lock");
        while self.should_keep_waiting(*signalled) {
            signalled = ready.wait(signalled).expect("thumbnail batch wake");
        }
        if self.stop.load(Ordering::Acquire) {
            return false;
        }
        *signalled = false;
        true
    }
}

struct GeneratedThumbnail {
    candidate: ThumbnailCandidate,
    state: ThumbnailState,
    subscribers: Vec<ThumbnailSubscriber>,
}

#[derive(Clone)]
struct ResultPublisherParts {
    cache: Arc<Mutex<ThumbnailCache>>,
    sink: BatchSink,
    clock: ThumbnailClock,
}

impl ResultPublisherParts {
    fn into_publisher(self) -> ResultPublisher {
        Arc::new(move |candidate, state, subscribers| {
            self.publish(GeneratedThumbnail {
                candidate,
                state,
                subscribers,
            });
        })
    }

    fn publish(&self, generated: GeneratedThumbnail) {
        let GeneratedThumbnail {
            candidate,
            state,
            subscribers,
        } = generated;
        let entry = ThumbnailCacheEntry {
            key: ThumbnailCacheKey(candidate.fingerprint.clone()),
            state: state.clone(),
        };
        let changed = self
            .cache
            .lock()
            .expect("thumbnail cache lock")
            .insert(entry, now_seconds());
        if !changed {
            return;
        }
        let events = subscribers
            .into_iter()
            .map(|subscriber| result_event(subscriber, candidate.clone(), state.clone()))
            .collect();
        if is_low_quality_ready(&state) {
            self.sink.emit(split_result_events(events));
            return;
        }
        self.sink.enqueue(events, (self.clock)());
    }
}

fn is_low_quality_ready(state: &ThumbnailState) -> bool {
    matches!(
        state,
        ThumbnailState::Ready {
            quality: crate::ipc::types::ThumbnailQuality::Low,
            ..
        }
    )
}

impl Drop for ThumbnailService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl BatchSink {
    fn enqueue(&self, events: Vec<ThumbnailResultEvent>, now_millis: u64) {
        let ready_batches = {
            let mut state = self.batches.lock().expect("thumbnail batches lock");
            state.events.extend(events);
            state.take_full_batches(now_millis)
        };
        self.emit(ready_batches);
        if self.has_pending_events() {
            *self.wake.0.lock().expect("thumbnail batch wake lock") = true;
            self.wake.1.notify_one();
        }
    }

    fn has_pending_events(&self) -> bool {
        let state = self.batches.lock().expect("thumbnail batches lock");
        !state.events.is_empty()
    }

    fn flush_due(&self, now_millis: u64) {
        let batch = self
            .batches
            .lock()
            .expect("thumbnail batches lock")
            .take_if_due(now_millis);
        self.emit(batch.into_iter().collect());
    }

    fn emit(&self, batches: Vec<Vec<ThumbnailResultEvent>>) {
        let Some(emit) = self.emitter.lock().expect("thumbnail emitter lock").clone() else {
            return;
        };
        for batch in batches {
            emit(batch);
        }
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
        let batch_full = batch.len() >= MAX_RESULTS_PER_BATCH
            || batch_bytes + event_bytes > MAX_RESULT_BYTES_PER_BATCH;
        let starts_new_batch = !batch.is_empty() && batch_full;
        if starts_new_batch {
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

fn full_batch_len(events: &[ThumbnailResultEvent]) -> usize {
    let mut batch_bytes = 0;
    let mut count = 0;
    for event in events {
        let event_bytes = result_event_weight(event);
        let batch_full = count >= MAX_RESULTS_PER_BATCH
            || batch_bytes + event_bytes > MAX_RESULT_BYTES_PER_BATCH;
        let overflows_started_batch = count > 0 && batch_full;
        if overflows_started_batch {
            break;
        }
        count += 1;
        batch_bytes += event_bytes;
    }
    count
}
