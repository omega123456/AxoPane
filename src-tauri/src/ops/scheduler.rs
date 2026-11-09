use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::execution::{run_operation, WorkerRuntime};
use super::service::Inner;
use super::{archive, worker, LockOperation, OpStatus, RemovedEmitter};
use crate::resource_coordinator::JobSpec;

/// Admits `id`'s declared resource keys through the shared resource
/// coordinator before actually running it. Used by both the initial
/// dispatch path (`spawn_worker`) and by `release_and_reschedule` once
/// previously-blocked work becomes eligible, so a rescheduled run contends
/// for the same global latency/throughput/CPU admission caps as a
/// first-dispatch run rather than only the legacy per-volume
/// `busy_volumes` gate. A denied admission (coordinator shut down or the
/// job was cancelled while queued for admission) marks the operation
/// cancelled instead of running it.
pub(super) fn admit_and_run_operation(
    id: String,
    inner: &Arc<Mutex<Inner>>,
    runtime: &WorkerRuntime,
) {
    let Some(admission) = admission_spec(&id, inner) else {
        return;
    };
    if runtime.coordinator.submit(admission).is_ok() {
        run_operation(id, inner, runtime);
        return;
    }
    mark_admission_denied(&id, inner);
}

fn admission_spec(id: &str, inner: &Arc<Mutex<Inner>>) -> Option<JobSpec> {
    let guard = inner.lock().expect("ops lock");
    let operation = guard.ops.get(id)?.lock_operation();
    let resources: Vec<String> = operation.volumes.iter().cloned().collect();
    let archive_cpu = archive::needs_cpu_admission(operation.kind);
    Some(worker::transfer_admission(resources, archive_cpu))
}

fn mark_admission_denied(id: &str, inner: &Arc<Mutex<Inner>>) {
    let Some(operation) = inner.lock().expect("ops lock").ops.get(id).cloned() else {
        return;
    };
    let mut operation = operation.lock_operation();
    operation.status = OpStatus::Cancelled;
    operation.runtime.error_message =
        Some("Transfer admission was cancelled or shut down.".to_string());
}

/// Once a worker finishes, free its volumes and try to schedule waiting work.
pub(super) fn release_and_reschedule(id: &str, inner: &Arc<Mutex<Inner>>, runtime: &WorkerRuntime) {
    inner.lock().expect("ops lock").release_volumes_of(id);

    // Dispatch any newly-eligible pending operations. Each still contends
    // for the shared resource coordinator's global admission caps, not just
    // the legacy per-volume `busy_volumes` gate freed above.
    let to_start = collect_startable(inner);
    for next in to_start {
        let inner = inner.clone();
        let runtime = runtime.clone();
        thread::spawn(move || {
            admit_and_run_operation(next.clone(), &inner, &runtime);
            release_and_reschedule(&next, &inner, &runtime);
        });
    }

    // Auto-remove completed/cancelled operations after the retention window.
    let removal = AutoRemoval {
        removed: runtime.removed.clone(),
        retention: runtime.retention,
    };
    schedule_auto_remove(id, inner, removal);
}

pub(super) fn collect_startable(inner: &Arc<Mutex<Inner>>) -> Vec<String> {
    let mut guard = inner.lock().expect("ops lock");
    let order: Vec<String> = guard.order.iter().cloned().collect();
    order
        .into_iter()
        .filter(|id| guard.claim_volumes_if_startable(id))
        .collect()
}

pub(super) struct AutoRemoval {
    pub(super) removed: Option<RemovedEmitter>,
    pub(super) retention: Duration,
}

pub(super) fn schedule_auto_remove(id: &str, inner: &Arc<Mutex<Inner>>, removal: AutoRemoval) {
    let AutoRemoval { removed, retention } = removal;
    let retired = inner.lock().expect("ops lock").is_retired(id);
    if !retired {
        return;
    }

    let inner = inner.clone();
    let id = id.to_string();
    thread::spawn(move || {
        thread::sleep(retention);
        let forgotten = inner.lock().expect("ops lock").forget_if_retired(&id);
        if !forgotten {
            return;
        }
        if let Some(emitter) = &removed {
            emitter(id);
        }
    });
}

impl Inner {
    fn claim_volumes_if_startable(&mut self, id: &str) -> bool {
        let Some(volumes) = self.startable_volumes(id) else {
            return false;
        };
        self.busy_volumes.extend(volumes);
        true
    }

    fn startable_volumes(&self, id: &str) -> Option<HashSet<String>> {
        let operation = self.ops.get(id)?.lock_operation();
        let volume_busy = operation
            .volumes
            .iter()
            .any(|volume| self.busy_volumes.contains(volume));
        let startable = operation.status == OpStatus::Pending && !volume_busy;
        startable.then(|| operation.volumes.clone())
    }

    fn release_volumes_of(&mut self, id: &str) {
        let volumes = self
            .ops
            .get(id)
            .map(|operation| operation.lock_operation().volumes.clone())
            .unwrap_or_default();
        for volume in &volumes {
            self.busy_volumes.remove(volume);
        }
    }

    fn is_retired(&self, id: &str) -> bool {
        self.ops.get(id).is_some_and(|operation| {
            matches!(
                operation.lock_operation().status,
                OpStatus::Completed | OpStatus::Cancelled
            )
        })
    }

    fn forget_if_retired(&mut self, id: &str) -> bool {
        if !self.is_retired(id) {
            return false;
        }
        self.ops.remove(id);
        self.signals.remove(id);
        self.order.retain(|entry| entry != id);
        true
    }
}
