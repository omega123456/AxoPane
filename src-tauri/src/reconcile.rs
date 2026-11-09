use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

#[derive(Default)]
struct ReconcileTarget {
    generation: AtomicU64,
    scheduled: AtomicBool,
}

#[derive(Default)]
pub struct ReconcileCoordinator {
    targets: Mutex<HashMap<String, Arc<ReconcileTarget>>>,
}

impl ReconcileCoordinator {
    fn target(&self, resource_key: &str) -> Arc<ReconcileTarget> {
        let mut targets = self.targets.lock().expect("reconcile targets lock");
        Arc::clone(
            targets
                .entry(resource_key.to_string())
                .or_insert_with(|| Arc::new(ReconcileTarget::default())),
        )
    }

    pub fn bump_generation(&self, resource_key: &str) -> u64 {
        self.target(resource_key)
            .generation
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }

    pub fn is_current(&self, resource_key: &str, generation: u64) -> bool {
        self.target(resource_key).generation.load(Ordering::SeqCst) == generation
    }

    pub fn request_reconcile(
        self: &Arc<Self>,
        resource: ReconcileResource,
        work: impl Fn(u64) + Send + Sync + 'static,
    ) -> u64 {
        let target = self.target(&resource.resource_key);
        let generation = target.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if target.scheduled.swap(true, Ordering::SeqCst) {
            return generation;
        }
        std::thread::spawn(move || {
            let drain = ReconcileDrain {
                target: &target,
                resource: &resource,
                work: &work,
            };
            drain.run_until_latest(generation);
        });

        generation
    }
}

pub struct ReconcileResource {
    pub coordinator: Arc<ResourceCoordinator>,
    pub resource_key: String,
}

struct ReconcileDrain<'drain, Work> {
    target: &'drain ReconcileTarget,
    resource: &'drain ReconcileResource,
    work: &'drain Work,
}

#[cfg(feature = "test-utils")]
pub fn generation_for_tests(coordinator: &ReconcileCoordinator) -> u64 {
    coordinator.target("test").generation.load(Ordering::SeqCst)
}

impl<Work: Fn(u64)> ReconcileDrain<'_, Work> {
    fn run_until_latest(&self, first_generation: u64) {
        let mut requested_generation = Some(first_generation);
        while let Some(generation) = requested_generation {
            self.run_if_current(generation);
            requested_generation = next_requested_generation(self.target, generation);
        }
    }

    fn run_if_current(&self, requested_generation: u64) {
        let resource_key = self.resource.resource_key.clone();
        let spec = JobSpec::new([JobClass::Latency], [resource_key]);
        let Ok(_permit) = self.resource.coordinator.submit(spec) else {
            return;
        };
        let current_generation = self.target.generation.load(Ordering::SeqCst);
        if current_generation == requested_generation {
            (self.work)(requested_generation);
        }
    }
}

fn next_requested_generation(target: &ReconcileTarget, requested_generation: u64) -> Option<u64> {
    let latest = target.generation.load(Ordering::SeqCst);
    if latest != requested_generation {
        return Some(latest);
    }
    target.scheduled.store(false, Ordering::SeqCst);
    let unchanged = target.generation.load(Ordering::SeqCst) == latest;
    if unchanged {
        return None;
    }
    let already_scheduled = target.scheduled.swap(true, Ordering::SeqCst);
    if already_scheduled {
        return None;
    }
    Some(target.generation.load(Ordering::SeqCst))
}
