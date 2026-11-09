use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use super::everything::{EverythingError, EverythingHandle};
use super::worker_pool::QueuedWork;
use super::{
    emit_non_empty, SizeService, SizeServiceInner, SizeSource, SizeStateKind, SizeUpdate,
    UpdateEmitter,
};
use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

/// Everything has process-global mutable query state.  This one gate is the
/// service's sole worker ownership boundary: no batch can query concurrently.
fn everything_worker_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

struct BatchJob {
    path: String,
    cancel: Arc<AtomicBool>,
}

struct EverythingBatch {
    jobs: Vec<BatchJob>,
    handle: Arc<EverythingHandle>,
    emitter: UpdateEmitter,
    inner: Arc<Mutex<SizeServiceInner>>,
}

impl SizeService {
    /// A path `register_job` could not schedule (the scheduler is at
    /// capacity) is excluded from the query and reported as a terminal
    /// `Error` at once, the same "reject, don't fabricate a phantom token"
    /// rule `enqueue_manual_job` applies for manual jobs.
    pub(super) fn spawn_everything_batch(&self, paths: Vec<String>, emitter: UpdateEmitter) {
        let Some(handle) = self.everything_handle() else {
            let updates = paths
                .into_iter()
                .map(|path| everything_error(&path))
                .collect();
            emit_non_empty(&emitter, updates);
            return;
        };

        let mut rejected_updates = Vec::new();
        let mut jobs = Vec::new();
        for path in paths {
            match self.register_job(&path, |_cancel| QueuedWork::EverythingRegistryOnly) {
                Some(cancel) => jobs.push(BatchJob { path, cancel }),
                None => rejected_updates.push(everything_error(&path)),
            }
        }
        emit_non_empty(&emitter, rejected_updates);
        if jobs.is_empty() {
            return;
        }

        let batch = EverythingBatch {
            jobs,
            handle,
            emitter,
            inner: Arc::clone(&self.inner),
        };
        let coordinator = Arc::clone(&self.coordinator);
        thread::spawn(move || batch.run(&coordinator));
    }
}

impl EverythingBatch {
    /// Everything itself is metadata/latency work. The process-global gate
    /// is intentionally acquired only after admission.
    fn run(self, coordinator: &ResourceCoordinator) {
        let _admission = coordinator.submit(JobSpec::new(
            [JobClass::Latency],
            ["everything".to_string()],
        ));
        let _worker = everything_worker_gate()
            .lock()
            .expect("everything worker gate");
        let requestable = self.requestable_paths();
        if requestable.is_empty() {
            self.complete_all();
            return;
        }

        self.announce_calculating(&requestable);
        let results = self
            .handle
            .query_folder_sizes(&requestable, &|| self.all_cancelled());
        let terminal_updates = self.finish_jobs(|path| everything_terminal_update(path, &results));
        emit_non_empty(&self.emitter, terminal_updates);
    }

    fn requestable_paths(&self) -> Vec<String> {
        self.jobs
            .iter()
            .filter(|job| !job.cancel.load(Ordering::Relaxed))
            .map(|job| job.path.clone())
            .collect()
    }

    /// The whole batch is abandoned once every one of its jobs has been
    /// cancelled (navigating away cancels them all at once).
    fn all_cancelled(&self) -> bool {
        self.jobs
            .iter()
            .all(|job| job.cancel.load(Ordering::Relaxed))
    }

    /// Announces the paths about to be queried, stopping the instant the
    /// batch is cancelled instead of emitting for every path.
    fn announce_calculating(&self, requestable: &[String]) {
        let calculating_updates = requestable
            .iter()
            .take_while(|_| !self.all_cancelled())
            .map(|path| {
                SizeUpdate::without_size(
                    path.clone(),
                    SizeStateKind::Calculating,
                    SizeSource::Everything,
                )
            })
            .collect();
        emit_non_empty(&self.emitter, calculating_updates);
    }

    fn complete_all(&self) {
        for job in &self.jobs {
            SizeService::complete_job(&self.inner, &job.path, &job.cancel);
        }
    }

    /// Completes every job, returning the terminal updates of those still
    /// wanted.
    fn finish_jobs(&self, terminal_update: impl Fn(&str) -> SizeUpdate) -> Vec<SizeUpdate> {
        self.jobs
            .iter()
            .filter_map(|job| self.finish_job(job, &terminal_update))
            .collect()
    }

    fn finish_job(
        &self,
        job: &BatchJob,
        terminal_update: &impl Fn(&str) -> SizeUpdate,
    ) -> Option<SizeUpdate> {
        let still_wanted = !job.cancel.load(Ordering::Relaxed);
        let update = still_wanted.then(|| terminal_update(&job.path));
        SizeService::complete_job(&self.inner, &job.path, &job.cancel);
        update
    }
}

fn everything_terminal_update(
    path: &str,
    results: &Result<HashMap<String, Option<u64>>, EverythingError>,
) -> SizeUpdate {
    let Ok(results) = results else {
        return everything_error(path);
    };
    let Some(size_bytes) = results.get(path).copied().flatten() else {
        return SizeUpdate::without_size(
            path.to_string(),
            SizeStateKind::Na,
            SizeSource::Everything,
        );
    };
    SizeUpdate::ready(path.to_string(), SizeSource::Everything, size_bytes)
}

fn everything_error(path: &str) -> SizeUpdate {
    SizeUpdate::without_size(
        path.to_string(),
        SizeStateKind::Error,
        SizeSource::Everything,
    )
}
