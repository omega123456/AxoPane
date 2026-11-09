use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::{
    scheduler, SizeService, SizeServiceInner, SizeSource, SizeStateKind, SizeUpdate, UpdateEmitter,
};
use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

pub(super) type ManualWork =
    Box<dyn FnOnce(String, Arc<AtomicBool>) -> Option<SizeUpdate> + Send + 'static>;

pub(super) struct ManualJob {
    emitter: UpdateEmitter,
    source: SizeSource,
    work: ManualWork,
    cancel: Arc<AtomicBool>,
}

pub(super) struct ManualJobRequest {
    pub(super) path: String,
    pub(super) source: SizeSource,
    pub(super) emitter: UpdateEmitter,
    pub(super) work: ManualWork,
}

pub(super) enum QueuedWork {
    Manual(ManualJob),
    EverythingRegistryOnly,
}

impl SizeService {
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
        let job = scheduler::ScheduledJob {
            path: path.to_string(),
            cancelled: Arc::clone(&cancel),
            payload: make_payload(Arc::clone(&cancel)),
        };
        if !inner.scheduler.schedule(job) {
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

    pub(super) fn enqueue_manual_job(&self, request: ManualJobRequest) {
        let ManualJobRequest {
            path,
            source,
            emitter,
            work,
        } = request;
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
            rejection_emitter(vec![SizeUpdate::without_size(
                path,
                SizeStateKind::Error,
                source,
            )]);
        }
    }
}

pub(super) fn spawn_manual_worker(
    scheduler: Arc<scheduler::SizeScheduler<QueuedWork>>,
    inner: Arc<Mutex<SizeServiceInner>>,
    coordinator: Arc<ResourceCoordinator>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let worker = ManualWorker {
            inner: &inner,
            coordinator: &coordinator,
        };
        worker.run(&scheduler);
    })
}

struct ManualWorker<'worker> {
    inner: &'worker Arc<Mutex<SizeServiceInner>>,
    coordinator: &'worker ResourceCoordinator,
}

impl ManualWorker<'_> {
    fn run(&self, scheduler: &scheduler::SizeScheduler<QueuedWork>) {
        while let Some(job) = scheduler.next() {
            self.run_queued(job);
        }
    }

    fn run_queued(&self, job: scheduler::ScheduledJob<QueuedWork>) {
        let QueuedWork::Manual(manual_job) = job.payload else {
            return;
        };
        self.run_job(job.path, manual_job);
    }

    fn run_job(&self, path: String, job: ManualJob) {
        let ManualJob {
            emitter,
            source,
            work,
            cancel,
        } = job;
        if cancel.load(Ordering::Relaxed) {
            SizeService::complete_job(self.inner, &path, &cancel);
            return;
        }
        let admission = self.coordinator.submit(JobSpec::new(
            [JobClass::Throughput],
            [format!("size:{path}")],
        ));
        let abandoned = admission.is_err() || cancel.load(Ordering::Relaxed);
        if abandoned {
            SizeService::complete_job(self.inner, &path, &cancel);
            return;
        }
        emitter(vec![SizeUpdate::without_size(
            path.clone(),
            SizeStateKind::Calculating,
            source,
        )]);
        let update = work(path.clone(), Arc::clone(&cancel));
        SizeService::complete_job(self.inner, &path, &cancel);
        if let Some(update) = update {
            emitter(vec![update]);
        }
    }
}
