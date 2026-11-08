use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::{
    scheduler, SizeService, SizeServiceInner, SizeSource, SizeStateKind, SizeUpdate, UpdateEmitter,
};
use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

/// The work a queued manual-size job performs once a worker claims it. Boxed
/// so it can travel through the scheduler's queue (as a job's `payload`)
/// instead of living only on the stack of a per-job spawned thread.
pub(super) type ManualWork =
    Box<dyn FnOnce(String, Arc<AtomicBool>) -> Option<SizeUpdate> + Send + 'static>;

/// Everything a manual-size worker needs once it claims a queued job: the
/// emitter to report `Calculating`/terminal events to, the traversal closure
/// itself, and the *external* cancellation token `SizeService::jobs` tracks
/// for this path (the sole identity `complete_job`'s `Arc::ptr_eq` check
/// authorizes against — see [`SizeService::register_job`]).
pub(super) struct ManualJob {
    emitter: UpdateEmitter,
    source: SizeSource,
    work: ManualWork,
    cancel: Arc<AtomicBool>,
}

/// The scheduler's queue payload. Every job registered through
/// `register_job` (manual *and* Everything-backed) occupies one entry in the
/// scheduler for coalescing/capacity purposes, but only manual jobs carry
/// actual work for a pool worker to run — an Everything-backed registration
/// is registry-only bookkeeping (`spawn_everything_batch` runs its own batch
/// query on its own thread, not via the pool), so it stores `None`.
pub(super) enum QueuedWork {
    Manual(ManualJob),
    EverythingRegistryOnly,
}

impl SizeService {
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
    pub(super) fn register_job(
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

    pub(super) fn complete_job(
        inner: &Arc<Mutex<SizeServiceInner>>,
        path: &str,
        cancel: &Arc<AtomicBool>,
    ) {
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
    pub(super) fn enqueue_manual_job(
        &self,
        path: String,
        source: SizeSource,
        emitter: UpdateEmitter,
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
            rejection_emitter(vec![SizeUpdate::new(
                path,
                SizeStateKind::Error,
                source,
                None,
            )]);
        }
    }
}

/// One long-lived manual-size worker thread body: pulls jobs from `scheduler`
/// one at a time (blocking on its condvar when empty) until the scheduler is
/// shut down. An adaptively sized pool of these is
/// what actually bounds how many OS threads folder-size traversal can ever
/// create, regardless of how many paths are requested at once.
///
/// An Everything-backed entry is registry-only bookkeeping owned by
/// `spawn_everything_batch`'s own thread, so a worker skips it.
pub(super) fn spawn_manual_worker(
    scheduler: Arc<scheduler::SizeScheduler<QueuedWork>>,
    inner: Arc<Mutex<SizeServiceInner>>,
    coordinator: Arc<ResourceCoordinator>,
) -> JoinHandle<()> {
    thread::spawn(move || run_manual_worker(&scheduler, &inner, &coordinator))
}

fn run_manual_worker(
    scheduler: &scheduler::SizeScheduler<QueuedWork>,
    inner: &Arc<Mutex<SizeServiceInner>>,
    coordinator: &ResourceCoordinator,
) {
    while let Some(job) = scheduler.next() {
        let QueuedWork::Manual(manual_job) = job.payload else {
            continue;
        };
        run_manual_job(inner, coordinator, job.path, manual_job);
    }
}

/// A job cancelled while queued does no filesystem work and emits nothing;
/// `Calculating` is emitted only once work actually begins. The job is
/// untracked *before* its terminal update so a request racing the delivery
/// starts a fresh job instead of coalescing onto a finished one.
fn run_manual_job(
    inner: &Arc<Mutex<SizeServiceInner>>,
    coordinator: &ResourceCoordinator,
    path: String,
    job: ManualJob,
) {
    let ManualJob {
        emitter,
        source,
        work,
        cancel,
    } = job;
    if cancel.load(Ordering::Relaxed) {
        SizeService::complete_job(inner, &path, &cancel);
        return;
    }
    let admission = coordinator.submit(JobSpec::new(
        [JobClass::Throughput],
        [format!("size:{path}")],
    ));
    if admission.is_err() || cancel.load(Ordering::Relaxed) {
        SizeService::complete_job(inner, &path, &cancel);
        return;
    }
    emitter(vec![SizeUpdate::new(
        path.clone(),
        SizeStateKind::Calculating,
        source,
        None,
    )]);
    let update = work(path.clone(), Arc::clone(&cancel));
    SizeService::complete_job(inner, &path, &cancel);
    if let Some(update) = update {
        emitter(vec![update]);
    }
}
