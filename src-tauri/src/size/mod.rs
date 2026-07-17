pub mod everything;
pub mod manual;
pub mod scheduler;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::resource_coordinator::{max_throughput_slots, JobClass, JobSpec, ResourceCoordinator};
use crate::volumes::{self, VolumeInfo};

use everything::{EverythingAvailability, EverythingHandle};

/// Long-lived manual-size worker threads. A handful of workers pull queued
/// jobs from the shared [`scheduler::SizeScheduler`] one at a time instead of
/// a thread being spawned per requested path — that is what actually bounds
/// how many OS threads a huge selection (or an "Items" sort over a folder
/// with tens of thousands of children) can create. The coordinator's
/// `Throughput` lane (see [`resource_coordinator::max_throughput_slots`])
/// still gates how many of these workers can be doing filesystem work at
/// once; this pool only bounds how many *threads exist*, not how many run
/// concurrently.
pub const DEFAULT_MANUAL_SIZE_TIMEOUT: Duration = Duration::from_secs(60);

/// The work a queued manual-size job performs once a worker claims it. Boxed
/// so it can travel through the scheduler's queue (as a job's `payload`)
/// instead of living only on the stack of a per-job spawned thread.
type ManualWork = Box<dyn FnOnce(String, Arc<AtomicBool>) -> Option<SizeUpdate> + Send + 'static>;

/// Everything a manual-size worker needs once it claims a queued job: the
/// emitter to report `Calculating`/terminal events to, the traversal closure
/// itself, and the *external* cancellation token `SizeService::jobs` tracks
/// for this path (the sole identity `complete_job`'s `Arc::ptr_eq` check
/// authorizes against — see [`SizeService::register_job`]).
struct ManualJob {
    emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
    source: SizeSource,
    work: ManualWork,
    cancel: Arc<AtomicBool>,
}

/// Everything has process-global mutable query state.  This one gate is the
/// service's sole worker ownership boundary: no batch can query concurrently.
fn everything_worker_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SizeStateKind {
    Unknown,
    Calculating,
    Ready,
    Error,
    Na,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SizeSource {
    Everything,
    Manual,
    Network,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EverythingStatusKind {
    Unsupported,
    Unavailable,
    NotReady,
    Available,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EverythingStatus {
    pub status: EverythingStatusKind,
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SizeUpdate {
    pub path: String,
    pub state: SizeStateKind,
    pub source: SizeSource,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeBackend {
    Everything,
    Manual,
    NetworkNa,
}

pub struct SizeService {
    inner: Arc<Mutex<SizeServiceInner>>,
    timeout: Duration,
    coordinator: Arc<ResourceCoordinator>,
    /// The bounded manual-size worker pool. Joined deterministically by
    /// `Drop` once the scheduler is shut down (which wakes every worker
    /// blocked in `scheduler.next()`).
    workers: Mutex<Vec<JoinHandle<()>>>,
}

/// The scheduler's queue payload. Every job registered through
/// `register_job` (manual *and* Everything-backed) occupies one entry in the
/// scheduler for coalescing/capacity purposes, but only manual jobs carry
/// actual work for a pool worker to run — an Everything-backed registration
/// is registry-only bookkeeping (`spawn_everything_batch` runs its own batch
/// query on its own thread, not via the pool), so it stores `None`.
enum QueuedWork {
    Manual(ManualJob),
    EverythingRegistryOnly,
}

struct SizeServiceInner {
    jobs: HashMap<String, Arc<AtomicBool>>,
    scheduler: Arc<scheduler::SizeScheduler<QueuedWork>>,
    everything: Option<Arc<EverythingHandle>>,
}

impl Default for SizeService {
    fn default() -> Self {
        Self::new(DEFAULT_MANUAL_SIZE_TIMEOUT)
    }
}

impl SizeService {
    pub fn new(timeout: Duration) -> Self {
        Self::with_resource_coordinator(timeout, Arc::new(ResourceCoordinator::new()))
    }

    /// Builds the service against the app-wide shared coordinator (mirrors
    /// `OpsService::with_resource_coordinator`) instead of constructing a
    /// private one. `lib.rs`'s setup constructs the shared coordinator once
    /// and injects it into every subsystem that submits admission-gated
    /// work, so folder-size traversal is fairly scheduled against the same
    /// throughput/CPU/latency lanes as copy/move and everything else.
    pub fn with_resource_coordinator(
        timeout: Duration,
        coordinator: Arc<ResourceCoordinator>,
    ) -> Self {
        let everything = EverythingHandle::load().ok().map(Arc::new);
        Self::build(
            timeout,
            everything,
            coordinator,
            scheduler::MAX_SIZE_METADATA_ENTRIES,
        )
    }

    #[cfg(feature = "test-utils")]
    pub fn with_everything_handle(timeout: Duration, handle: Option<EverythingHandle>) -> Self {
        Self::with_everything_handle_and_coordinator(
            timeout,
            handle,
            Arc::new(ResourceCoordinator::new()),
        )
    }

    /// Test-only constructor that lets a test inject both a fake Everything
    /// handle and a specific coordinator (e.g. the app-wide shared one, to
    /// exercise fairness across subsystems), following the same convention
    /// as `OpsService`'s test-utils constructors.
    #[cfg(feature = "test-utils")]
    pub fn with_everything_handle_and_coordinator(
        timeout: Duration,
        handle: Option<EverythingHandle>,
        coordinator: Arc<ResourceCoordinator>,
    ) -> Self {
        Self::build(
            timeout,
            handle.map(Arc::new),
            coordinator,
            scheduler::MAX_SIZE_METADATA_ENTRIES,
        )
    }

    /// Test-only constructor that additionally overrides the scheduler's
    /// job-count capacity (production always uses
    /// [`scheduler::MAX_SIZE_METADATA_ENTRIES`]), so a test can deterministically
    /// exercise the "scheduler rejects a new job because it is full" branch
    /// of `register_job` without needing to queue thousands of real paths.
    #[cfg(feature = "test-utils")]
    pub fn with_capacity_for_tests(
        timeout: Duration,
        coordinator: Arc<ResourceCoordinator>,
        capacity: usize,
    ) -> Self {
        Self::build(timeout, None, coordinator, capacity)
    }

    fn build(
        timeout: Duration,
        everything: Option<Arc<EverythingHandle>>,
        coordinator: Arc<ResourceCoordinator>,
        scheduler_capacity: usize,
    ) -> Self {
        let scheduler = Arc::new(scheduler::SizeScheduler::new(scheduler_capacity));
        let inner = Arc::new(Mutex::new(SizeServiceInner {
            jobs: HashMap::new(),
            scheduler: Arc::clone(&scheduler),
            everything,
        }));

        let workers = (0..max_throughput_slots())
            .map(|_| {
                spawn_manual_worker(
                    Arc::clone(&scheduler),
                    Arc::clone(&inner),
                    Arc::clone(&coordinator),
                )
            })
            .collect();

        Self {
            inner,
            timeout,
            coordinator,
            workers: Mutex::new(workers),
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn manual_worker_count_for_tests(&self) -> usize {
        self.workers
            .lock()
            .expect("size service workers lock")
            .len()
    }

    pub fn everything_status(&self) -> EverythingStatus {
        let inner = self.inner.lock().expect("size service lock");
        let availability = inner
            .everything
            .as_ref()
            .map(|handle| handle.availability());

        match availability {
            Some(EverythingAvailability::Available) => EverythingStatus {
                status: EverythingStatusKind::Available,
                is_available: true,
            },
            #[cfg(windows)]
            Some(EverythingAvailability::NotReady) => EverythingStatus {
                status: EverythingStatusKind::NotReady,
                is_available: false,
            },
            Some(EverythingAvailability::Unavailable) | None if cfg!(windows) => EverythingStatus {
                status: EverythingStatusKind::Unavailable,
                is_available: false,
            },
            _ => EverythingStatus {
                status: EverythingStatusKind::Unsupported,
                is_available: false,
            },
        }
    }

    pub fn choose_backend_for_path(&self, path: &Path, volumes: &[VolumeInfo]) -> SizeBackend {
        if volumes::path_is_network(path, volumes) {
            return SizeBackend::NetworkNa;
        }

        let inner = self.inner.lock().expect("size service lock");
        if inner
            .everything
            .as_ref()
            .is_some_and(|handle| handle.availability() == EverythingAvailability::Available)
        {
            SizeBackend::Everything
        } else {
            SizeBackend::Manual
        }
    }

    pub fn request_paths<F>(&self, paths: Vec<String>, emitter: F)
    where
        F: Fn(Vec<SizeUpdate>) + Send + Sync + 'static,
    {
        let emitter = Arc::new(emitter);
        let volumes = volumes::list_volumes();
        self.request_paths_with_volumes(paths, volumes, emitter);
    }

    pub fn cancel(&self, path: &str) -> bool {
        let mut inner = self.inner.lock().expect("size service lock");
        if let Some(cancel) = inner.jobs.remove(path) {
            cancel.store(true, Ordering::Relaxed);
            inner.scheduler.cancel(path);
            true
        } else {
            false
        }
    }

    /// Cancels every in-flight job in `paths`, returning how many were actually
    /// running. Used to abandon a folder's pending size jobs when the pane
    /// navigates away, without disturbing jobs for any other path.
    pub fn cancel_many(&self, paths: &[String]) -> usize {
        let mut inner = self.inner.lock().expect("size service lock");
        paths
            .iter()
            .filter(|path| {
                inner.jobs.remove(*path).is_some_and(|cancel| {
                    cancel.store(true, Ordering::Relaxed);
                    inner.scheduler.cancel(path);
                    true
                })
            })
            .count()
    }

    pub fn request_path_with_volumes(
        &self,
        path: String,
        volumes: Vec<VolumeInfo>,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
    ) {
        self.request_paths_with_volumes(vec![path], volumes, emitter);
    }

    pub fn request_paths_with_volumes(
        &self,
        paths: Vec<String>,
        volumes: Vec<VolumeInfo>,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
    ) {
        let mut everything_paths = Vec::new();
        let mut network_updates = Vec::new();

        for path in paths {
            let backend = self.choose_backend_for_path(Path::new(&path), &volumes);

            match backend {
                SizeBackend::NetworkNa => {
                    network_updates.push(SizeUpdate {
                        path,
                        state: SizeStateKind::Na,
                        source: SizeSource::Network,
                        size_bytes: None,
                    });
                }
                SizeBackend::Everything => everything_paths.push(path),
                SizeBackend::Manual => {
                    let timeout = self.timeout;
                    self.enqueue_manual_job(
                        path,
                        SizeSource::Manual,
                        emitter.clone(),
                        Box::new(move |job_path, cancel| {
                            match manual::calculate(Path::new(&job_path), &cancel, timeout) {
                                Ok(size_bytes) => Some(SizeUpdate {
                                    path: job_path,
                                    state: SizeStateKind::Ready,
                                    source: SizeSource::Manual,
                                    size_bytes: Some(size_bytes),
                                }),
                                Err(manual::ManualSizeError::Timeout) => Some(SizeUpdate {
                                    path: job_path,
                                    state: SizeStateKind::Na,
                                    source: SizeSource::Manual,
                                    size_bytes: None,
                                }),
                                Err(manual::ManualSizeError::Cancelled) => None,
                                Err(_) => Some(SizeUpdate {
                                    path: job_path,
                                    state: SizeStateKind::Error,
                                    source: SizeSource::Manual,
                                    size_bytes: None,
                                }),
                            }
                        }),
                    );
                }
            }
        }

        if !network_updates.is_empty() {
            emitter(network_updates);
        }

        if !everything_paths.is_empty() {
            self.spawn_everything_batch(everything_paths, emitter);
        }
    }

    fn spawn_everything_batch(
        &self,
        paths: Vec<String>,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
    ) {
        let handle = {
            let inner = self.inner.lock().expect("size service lock");
            inner.everything.clone()
        };
        let Some(handle) = handle else {
            let updates = paths
                .into_iter()
                .map(|path| SizeUpdate {
                    path,
                    state: SizeStateKind::Error,
                    source: SizeSource::Everything,
                    size_bytes: None,
                })
                .collect::<Vec<_>>();
            if !updates.is_empty() {
                emitter(updates);
            }
            return;
        };

        // A path `register_job` could not schedule (the scheduler is at
        // capacity) is treated as already-cancelled for this batch: it is
        // excluded from the Everything query below (so it does no work and
        // is not silently dropped from tracking) and reported as a terminal
        // `Error` immediately, since there is no real job to ever complete
        // it. This is the same "reject, don't fabricate a phantom token"
        // rule `enqueue_manual_job` applies for manual jobs.
        let mut rejected_updates = Vec::new();
        let jobs = paths
            .into_iter()
            .filter_map(|path| {
                match self.register_job(&path, |_cancel| QueuedWork::EverythingRegistryOnly) {
                    Some(cancel) => Some((path, cancel)),
                    None => {
                        rejected_updates.push(SizeUpdate {
                            path,
                            state: SizeStateKind::Error,
                            source: SizeSource::Everything,
                            size_bytes: None,
                        });
                        None
                    }
                }
            })
            .collect::<Vec<_>>();
        if !rejected_updates.is_empty() {
            emitter(rejected_updates);
        }
        if jobs.is_empty() {
            return;
        }
        let inner = Arc::clone(&self.inner);
        let coordinator = Arc::clone(&self.coordinator);

        thread::spawn(move || {
            // Everything itself is metadata/latency work. The process-global
            // gate below is intentionally acquired only after admission.
            let _admission = coordinator.submit(JobSpec::new(
                [JobClass::Latency],
                ["everything".to_string()],
            ));
            let _worker = everything_worker_gate()
                .lock()
                .expect("everything worker gate");
            let requestable = jobs
                .iter()
                .filter_map(|(path, cancel)| {
                    (!cancel.load(Ordering::Relaxed)).then_some(path.clone())
                })
                .collect::<Vec<_>>();

            if requestable.is_empty() {
                for (path, cancel) in jobs {
                    SizeService::complete_job(&inner, &path, &cancel);
                }
                return;
            }

            // The whole batch is abandoned once every one of its jobs has been
            // cancelled (navigating away cancels them all at once). `.all`
            // short-circuits on the first still-active job, so this is O(1) in
            // the common case and only scans fully when actually cancelled.
            // Scoped in a block so the borrow of `jobs` ends before the result
            // loop below consumes it.
            let results = {
                let batch_cancelled = || {
                    jobs.iter()
                        .all(|(_, cancel)| cancel.load(Ordering::Relaxed))
                };

                // Announce the jobs we're actually about to query (cancelled
                // ones are skipped above), bailing out of the flood the instant
                // the batch is cancelled instead of emitting for every path.
                let mut calculating_updates = Vec::with_capacity(requestable.len());
                for path in &requestable {
                    if batch_cancelled() {
                        break;
                    }
                    calculating_updates.push(SizeUpdate {
                        path: path.clone(),
                        state: SizeStateKind::Calculating,
                        source: SizeSource::Everything,
                        size_bytes: None,
                    });
                }
                if !calculating_updates.is_empty() {
                    emitter(calculating_updates);
                }

                handle.query_folder_sizes(&requestable, &batch_cancelled)
            };

            match results {
                Ok(results) => {
                    let mut terminal_updates = Vec::new();
                    for (path, cancel) in jobs {
                        if cancel.load(Ordering::Relaxed) {
                            SizeService::complete_job(&inner, &path, &cancel);
                            continue;
                        }

                        terminal_updates.push(SizeUpdate {
                            path: path.clone(),
                            state: results
                                .get(&path)
                                .copied()
                                .flatten()
                                .map(|_| SizeStateKind::Ready)
                                .unwrap_or(SizeStateKind::Na),
                            source: SizeSource::Everything,
                            size_bytes: results.get(&path).copied().flatten(),
                        });
                        SizeService::complete_job(&inner, &path, &cancel);
                    }
                    if !terminal_updates.is_empty() {
                        emitter(terminal_updates);
                    }
                }
                Err(_) => {
                    let mut terminal_updates = Vec::new();
                    for (path, cancel) in jobs {
                        if cancel.load(Ordering::Relaxed) {
                            SizeService::complete_job(&inner, &path, &cancel);
                            continue;
                        }

                        terminal_updates.push(SizeUpdate {
                            path: path.clone(),
                            state: SizeStateKind::Error,
                            source: SizeSource::Everything,
                            size_bytes: None,
                        });
                        SizeService::complete_job(&inner, &path, &cancel);
                    }
                    if !terminal_updates.is_empty() {
                        emitter(terminal_updates);
                    }
                }
            }
        });
    }

    /// Registers a cancellation handle for `path`, superseding any prior job
    /// for the same path. Deliberately cheap and side-effect free: it does
    /// **not** emit a `Calculating` event, so enqueuing thousands of jobs is a
    /// tight, fast loop rather than thousands of synchronous IPC posts. The
    /// `Calculating` event is emitted lazily by the worker once it actually
    /// starts the job (see [`spawn_manual_worker`] and
    /// [`SizeService::spawn_everything_batch`]), which means a job cancelled
    /// before a worker reaches it produces no events and does no work at all.
    ///
    /// Returns `None` when the scheduler could not admit `path` as a new
    /// entry (it is at capacity — [`scheduler::MAX_SIZE_METADATA_ENTRIES`]).
    /// Callers must treat `None` as "this job will never run or complete" —
    /// they must not fabricate a disconnected token and hand it out as if it
    /// were tracked, since nothing would ever call `complete_job` for it and
    /// the caller-visible request would silently vanish with no terminal
    /// event. Both call sites (`enqueue_manual_job` and
    /// `spawn_everything_batch`) instead emit an immediate terminal `Error`
    /// for a rejected path.
    /// `make_payload` receives the freshly created external cancel token so
    /// it can embed it in the queued payload (see [`ManualJob::cancel`]) —
    /// this is what makes the token stored in `inner.jobs`, the token
    /// embedded in the scheduler's queued payload, and the token
    /// [`SizeService::complete_job`] is eventually called with all the
    /// *same* `Arc`, closing the phantom-token gap described on
    /// [`SizeService::register_job`]. Only called while genuinely holding
    /// the new scheduler slot (never for the coalesced-onto-existing-job
    /// branch), so it never fires for a request that will not own the slot.
    fn register_job(
        &self,
        path: &str,
        make_payload: impl FnOnce(Arc<AtomicBool>) -> QueuedWork,
    ) -> Option<Arc<AtomicBool>> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut inner = self.inner.lock().expect("size service lock");
        if let Some(existing) = inner.jobs.get(path) {
            return Some(Arc::clone(existing));
        }
        let payload = make_payload(Arc::clone(&cancel));
        if !inner
            .scheduler
            .schedule(path.to_string(), Arc::clone(&cancel), payload)
        {
            return None;
        }
        inner.jobs.insert(path.to_string(), Arc::clone(&cancel));
        Some(cancel)
    }

    fn complete_job(inner: &Arc<Mutex<SizeServiceInner>>, path: &str, cancel: &Arc<AtomicBool>) {
        let mut locked = inner.lock().expect("size service lock");
        if locked
            .jobs
            .get(path)
            .is_some_and(|registered| Arc::ptr_eq(registered, cancel))
        {
            locked.jobs.remove(path);
        }
        locked.scheduler.complete(path, cancel);
    }

    /// Registers `path` and, once it is genuinely scheduled, hands its work
    /// closure to the shared manual-size scheduler queue rather than
    /// spawning a dedicated OS thread. The bounded adaptive worker pool (see
    /// [`spawn_manual_worker`]) claims and executes it.
    fn enqueue_manual_job(
        &self,
        path: String,
        source: SizeSource,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
        work: ManualWork,
    ) {
        // If this path is already tracked (either still queued or actively
        // executing under a pool worker), `register_job` returns the
        // existing token and this fresh `work`/`emitter` are simply dropped
        // — the coalescing contract keeps the original in-flight job's
        // payload as-is. Otherwise the payload below becomes the scheduler
        // entry for a brand-new job, and a pool worker will claim it.
        let rejection_emitter = Arc::clone(&emitter);
        let registered = self.register_job(&path, move |cancel| {
            QueuedWork::Manual(ManualJob {
                emitter,
                source,
                work,
                cancel,
            })
        });

        if registered.is_none() {
            // Scheduler at capacity: no job was tracked anywhere, so there is
            // no in-flight work to coalesce onto and nothing will ever call
            // `complete_job` for this path. Report the terminal state
            // immediately instead of leaving the caller waiting forever.
            rejection_emitter(vec![SizeUpdate {
                path,
                state: SizeStateKind::Error,
                source,
                size_bytes: None,
            }]);
        }
    }
}

/// One long-lived manual-size worker thread body: pulls jobs from `scheduler`
/// one at a time (blocking on its condvar when empty) until the scheduler is
/// shut down. An adaptively sized pool of these is
/// what actually bounds how many OS threads folder-size traversal can ever
/// create, regardless of how many paths are requested at once.
fn spawn_manual_worker(
    scheduler: Arc<scheduler::SizeScheduler<QueuedWork>>,
    inner: Arc<Mutex<SizeServiceInner>>,
    coordinator: Arc<ResourceCoordinator>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Some(job) = scheduler.next() {
            // `next()` dequeues from the single FIFO shared with
            // Everything-backed registrations (both go through
            // `register_job`/`schedule` for coalescing/capacity purposes —
            // see `QueuedWork`). An Everything-backed entry is registry-only
            // bookkeeping owned entirely by `spawn_everything_batch`'s own
            // thread, which completes it directly by path without ever
            // dequeuing. Popping it here off the FIFO early is harmless
            // (nothing else dequeues), but this worker must not claim,
            // execute, or complete it — just move on to the next job.
            let QueuedWork::Manual(ManualJob {
                emitter,
                source,
                work,
                cancel,
            }) = job.payload
            else {
                continue;
            };
            let path = job.path;

            // A job may sit queued behind thousands of others; if it was
            // cancelled before a worker reached it (e.g. the pane navigated
            // away from a folder with tens of thousands of subfolders), skip
            // the filesystem work entirely instead of `stat`ing and walking a
            // folder whose result will be discarded. The `Calculating` event
            // is emitted here — only once work actually begins — so a
            // cancelled backlog stays completely silent. `next()` above
            // already claimed this path's payload (removing it from the
            // queue), so the only remaining check is whether the caller's
            // external token was flipped in the window between queuing and
            // this worker reaching it.
            if !cancel.load(Ordering::Relaxed) {
                let admission = coordinator.submit(JobSpec::new(
                    [JobClass::Throughput],
                    [format!("size:{path}")],
                ));
                if admission.is_err() || cancel.load(Ordering::Relaxed) {
                    SizeService::complete_job(&inner, &path, &cancel);
                    continue;
                }
                emitter(vec![SizeUpdate {
                    path: path.clone(),
                    state: SizeStateKind::Calculating,
                    source,
                    size_bytes: None,
                }]);
                let update = work(path.clone(), Arc::clone(&cancel));
                // Untrack the job *before* emitting its terminal update, not
                // after: a request for this exact path that lands in the
                // narrow window between the walk finishing and this worker
                // getting back to `complete_job` would otherwise coalesce
                // onto a job whose terminal event has already fired (through
                // this call's `emitter`, not the new caller's), so the new
                // caller would never observe completion. Completing first
                // means a request arriving after the walk finishes always
                // sees an untracked path and starts a fresh job with its own
                // emitter, matching the coalescing contract's intent: only
                // a request that overlaps *genuinely in-flight* work
                // coalesces, not one that loses a race with delivery of the
                // terminal event.
                SizeService::complete_job(&inner, &path, &cancel);
                if let Some(update) = update {
                    emitter(vec![update]);
                }
                continue;
            }
            SizeService::complete_job(&inner, &path, &cancel);
        }
    })
}

impl Drop for SizeService {
    fn drop(&mut self) {
        // Wakes every worker parked in `scheduler.next()` (it returns `None`
        // once `shutdown` has been called and the queue is drained), so the
        // pool below joins deterministically instead of leaking blocked
        // threads. The shared `ResourceCoordinator` is intentionally left
        // alone here — unlike `WatchRuntime`, which owns a private
        // coordinator, `SizeService` only holds a reference to the app-wide
        // shared one and must not shut it down out from under other
        // subsystems still using it.
        {
            let inner = self.inner.lock().expect("size service lock");
            inner.scheduler.shutdown();
        }
        for worker in self
            .workers
            .get_mut()
            .expect("size service workers lock")
            .drain(..)
        {
            let _ = worker.join();
        }
    }
}

pub fn size_path_from_string(path: &str) -> PathBuf {
    PathBuf::from(path)
}
