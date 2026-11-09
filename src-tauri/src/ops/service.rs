use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(feature = "test-utils")]
use super::execution::run_operation;
use super::execution::WorkerRuntime;
use super::scheduler::{
    admit_and_run_operation, collect_startable, release_and_reschedule, schedule_auto_remove,
    AutoRemoval,
};
use super::status::Cancellation;
use super::{
    volume_root_for, ConflictDecision, ConflictEmitter, InstantNow, LockOperation, OpKind,
    OpRuntime, OpSnapshot, OpState, OpStatus, ProgressEmitter, RemovedEmitter, StartOpRequest,
    DEFAULT_COMPLETED_RETENTION, DEFAULT_RATE_WINDOW,
};
use crate::volumes::VolumeInfo;

pub(super) struct Inner {
    pub(super) ops: HashMap<String, Arc<Mutex<OpState>>>,
    /// Per-operation wakeup signal. Each operation owns its own `Condvar` so that a
    /// parked worker always pairs the same `Condvar` with the same operation mutex —
    /// `Condvar` panics on platforms (e.g. macOS pthread) if a single instance
    /// is waited on with two different mutexes, which happened when one shared
    /// `Condvar` served every operation's per-op `OpState` mutex.
    pub(super) signals: HashMap<String, Arc<Condvar>>,
    /// Submission/scheduling order (also the reorder order for pending items).
    pub(super) order: VecDeque<String>,
    /// Volumes currently occupied by an active operation.
    pub(super) busy_volumes: HashSet<String>,
    pub(super) workers: Vec<JoinHandle<()>>,
}

/// The queue engine. Held as Tauri managed state.
pub struct OpsService {
    inner: Arc<Mutex<Inner>>,
    next_id: AtomicU64,
    progress_emitter: Mutex<Option<ProgressEmitter>>,
    conflict_emitter: Mutex<Option<ConflictEmitter>>,
    removed_emitter: Mutex<Option<RemovedEmitter>>,
    completed_retention: Duration,
    rate_window: Duration,
    instant_now: InstantNow,
    volumes: Mutex<Vec<VolumeInfo>>,
    coordinator: Arc<crate::resource_coordinator::ResourceCoordinator>,
}

impl Default for OpsService {
    fn default() -> Self {
        Self::new(DEFAULT_COMPLETED_RETENTION)
    }
}

impl OpsService {
    pub fn new(completed_retention: Duration) -> Self {
        Self::with_rate_window(completed_retention, DEFAULT_RATE_WINDOW)
    }

    pub fn with_rate_window(completed_retention: Duration, rate_window: Duration) -> Self {
        Self::with_rate_window_and_coordinator(
            completed_retention,
            rate_window,
            Arc::new(crate::resource_coordinator::ResourceCoordinator::new()),
        )
    }

    pub fn with_resource_coordinator(
        coordinator: Arc<crate::resource_coordinator::ResourceCoordinator>,
    ) -> Self {
        Self::with_rate_window_and_coordinator(
            DEFAULT_COMPLETED_RETENTION,
            DEFAULT_RATE_WINDOW,
            coordinator,
        )
    }

    fn with_rate_window_and_coordinator(
        completed_retention: Duration,
        rate_window: Duration,
        coordinator: Arc<crate::resource_coordinator::ResourceCoordinator>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::new())),
            next_id: AtomicU64::new(1),
            progress_emitter: Mutex::new(None),
            conflict_emitter: Mutex::new(None),
            removed_emitter: Mutex::new(None),
            completed_retention,
            rate_window,
            instant_now: Arc::new(Instant::now),
            volumes: Mutex::new(crate::volumes::list_volumes()),
            coordinator,
        }
    }

    /// Override the volume table used for identity (tests inject deterministic data).
    pub fn set_volumes(&self, volumes: Vec<VolumeInfo>) {
        *self.volumes.lock().expect("volumes lock") = volumes;
    }

    pub fn set_progress_emitter(&self, emitter: ProgressEmitter) {
        *self.progress_emitter.lock().expect("progress emitter lock") = Some(emitter);
    }

    pub fn set_conflict_emitter(&self, emitter: ConflictEmitter) {
        *self.conflict_emitter.lock().expect("conflict emitter lock") = Some(emitter);
    }

    pub fn set_removed_emitter(&self, emitter: RemovedEmitter) {
        *self.removed_emitter.lock().expect("removed emitter lock") = Some(emitter);
    }

    #[cfg(feature = "test-utils")]
    pub fn set_instant_now_for_tests(&mut self, instant_now: InstantNow) {
        self.instant_now = instant_now;
    }

    pub fn schedule_auto_remove_for(&self, id: &str) {
        let removed = self
            .removed_emitter
            .lock()
            .expect("removed emitter lock")
            .clone();
        let removal = AutoRemoval {
            removed,
            retention: self.completed_retention,
        };
        schedule_auto_remove(id, &self.inner, removal);
    }

    pub fn emit_progress(&self, operation: &OpState) {
        if let Some(emitter) = self
            .progress_emitter
            .lock()
            .expect("progress emitter lock")
            .clone()
        {
            emitter(operation.progress());
        }
    }

    pub fn volumes_for(&self, paths: &[&Path]) -> HashSet<String> {
        let volumes = self.volumes.lock().expect("volumes lock").clone();
        let mut roots = HashSet::new();
        for path in paths {
            roots.insert(volume_root_for(path, &volumes));
        }
        roots
    }

    /// Enqueue a new copy/move operation. Returns its id.
    pub fn start_op(&self, request: StartOpRequest) -> String {
        let id = format!("op-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let destination_dir = PathBuf::from(&request.destination_dir);

        let mut source_paths: Vec<PathBuf> = request
            .items
            .iter()
            .map(|item| PathBuf::from(&item.source_path))
            .collect();
        // A delete has no destination; lock only the source volume(s).
        if !matches!(request.kind, OpKind::Delete) {
            source_paths.push(destination_dir.clone());
        }
        let path_refs: Vec<&Path> = source_paths.iter().map(PathBuf::as_path).collect();
        let operation_volumes = self.volumes_for(&path_refs);

        let total_bytes: u64 = request.items.iter().map(|item| item.size_bytes).sum();
        let total_items = request.items.len() as u64;

        let state = OpState {
            id: id.clone(),
            kind: request.kind,
            destination_dir,
            items: request.items,
            volumes: operation_volumes,
            status: OpStatus::Pending,
            total_items,
            total_bytes,
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            skip: Arc::new(AtomicBool::new(false)),
            runtime: OpRuntime::default(),
        };

        {
            let mut inner = self.inner.lock().expect("ops lock");
            let operation = Arc::new(Mutex::new(state));
            inner.ops.insert(id.clone(), operation.clone());
            inner.signals.insert(id.clone(), Arc::new(Condvar::new()));
            inner.order.push_back(id.clone());
            self.emit_progress(&operation.lock_operation());
        }

        self.schedule();
        id
    }

    fn schedule(self: &OpsService) {
        for id in collect_startable(&self.inner) {
            self.spawn_worker(id);
        }
    }

    fn worker_runtime(&self) -> WorkerRuntime {
        WorkerRuntime {
            progress: self
                .progress_emitter
                .lock()
                .expect("progress emitter lock")
                .clone(),
            conflict: self
                .conflict_emitter
                .lock()
                .expect("conflict emitter lock")
                .clone(),
            removed: self
                .removed_emitter
                .lock()
                .expect("removed emitter lock")
                .clone(),
            retention: self.completed_retention,
            rate_window: self.rate_window,
            instant_now: self.instant_now.clone(),
            coordinator: Arc::clone(&self.coordinator),
        }
    }

    fn spawn_worker(self: &OpsService, id: String) {
        let inner = self.inner.clone();
        let runtime = self.worker_runtime();
        let handle = thread::spawn(move || {
            // All source/destination resource keys are admitted as one
            // duplicate-collapsed set through the shared resource
            // coordinator, whether this is `id`'s first dispatch or a
            // rescheduled run after another operation released its
            // volumes — see `admit_and_run_operation`.
            admit_and_run_operation(id.clone(), &inner, &runtime);
            release_and_reschedule(&id, &inner, &runtime);
        });

        let mut inner = self.inner.lock().expect("ops lock");
        inner.workers.retain(|worker| !worker.is_finished());
        inner.workers.push(handle);
    }

    pub fn pause_op(&self, id: &str) {
        let Some(operation) = self.operation(id) else {
            return;
        };
        let mut guard = operation.lock_operation();
        let Ok(next) = guard.status.pause() else {
            return;
        };
        guard.pause.store(true, Ordering::Relaxed);
        guard.status = next;
        self.emit_progress(&guard);
    }

    pub fn resume_op(&self, id: &str) {
        if self.resume_locked(id) {
            self.notify_waiters(id);
        }
    }

    fn resume_locked(&self, id: &str) -> bool {
        let Some(operation) = self.operation(id) else {
            return false;
        };
        let mut guard = operation.lock_operation();
        let Ok(next) = guard.status.resume() else {
            return false;
        };
        guard.pause.store(false, Ordering::Relaxed);
        guard.status = next;
        self.emit_progress(&guard);
        true
    }

    pub fn cancel_op(&self, id: &str) {
        let Some(cancellation) = self.cancel_locked(id) else {
            return;
        };
        match cancellation {
            Cancellation::BeforeStart => self.schedule_auto_remove_for(id),
            Cancellation::WhileRunning => self.notify_waiters(id),
        }
    }

    fn cancel_locked(&self, id: &str) -> Option<Cancellation> {
        let operation = self.operation(id)?;
        let mut guard = operation.lock_operation();
        let cancellation = guard.status.cancel().ok()?;
        match cancellation {
            Cancellation::BeforeStart => guard.runtime.completed_at = Some(Instant::now()),
            Cancellation::WhileRunning => guard.request_cancel(),
        }
        guard.status = cancellation.next_status();
        self.emit_progress(&guard);
        Some(cancellation)
    }

    /// Skip one item only from a paused operation, then leave the operation paused.
    pub fn skip_op(&self, id: &str) {
        if self.skip_locked(id) {
            self.notify_waiters(id);
        }
    }

    fn skip_locked(&self, id: &str) -> bool {
        let Some(operation) = self.operation(id) else {
            return false;
        };
        let mut guard = operation.lock_operation();
        let Ok(next) = guard.status.skip() else {
            return false;
        };
        guard.skip.store(true, Ordering::Relaxed);
        guard.cancel.store(true, Ordering::Relaxed);
        guard.pause.store(false, Ordering::Relaxed);
        guard.status = next;
        self.emit_progress(&guard);
        true
    }

    fn notify_waiters(&self, id: &str) {
        if let Some(signal) = self.signal(id) {
            signal.notify_all();
        }
    }

    /// Reorder *pending* operations. Unknown / non-pending ids are ignored; any
    /// pending ids not named keep their relative order at the back.
    pub fn reorder_ops(&self, ids: &[String]) {
        let mut inner = self.inner.lock().expect("ops lock");
        let pending = inner.pending_ids();
        let requested: Vec<String> = ids
            .iter()
            .filter(|id| pending.contains(*id))
            .cloned()
            .collect();
        inner.order = reordered_queue(&inner.order, &pending, requested);
    }

    pub fn resolve_conflict(&self, id: &str, decision: ConflictDecision) {
        if self.resolve_conflict_locked(id, decision) {
            self.notify_waiters(id);
        }
    }

    fn resolve_conflict_locked(&self, id: &str, decision: ConflictDecision) -> bool {
        let Some(operation) = self.operation(id) else {
            return false;
        };
        let mut guard = operation.lock_operation();
        let Ok(next) = guard.status.resolve_conflict() else {
            return false;
        };
        guard.runtime.conflict_resolution = Some(decision.resolution);
        guard.runtime.rename_to = decision.rename_to;
        if decision.apply_to_all {
            guard.runtime.apply_to_all = Some(decision.resolution);
        }
        guard.runtime.conflict = None;
        guard.status = next;
        self.emit_progress(&guard);
        true
    }

    /// Retry a failed operation by re-queuing it.
    pub fn retry_op(&self, id: &str) {
        if self.retry_locked(id) {
            self.schedule();
        }
    }

    fn retry_locked(&self, id: &str) -> bool {
        let Some(operation) = self.operation(id) else {
            return false;
        };
        let mut guard = operation.lock_operation();
        let Ok(next) = guard.status.retry() else {
            return false;
        };
        guard.status = next;
        guard.reset_runtime_state();
        guard.cancel = Arc::new(AtomicBool::new(false));
        guard.pause = Arc::new(AtomicBool::new(false));
        guard.skip = Arc::new(AtomicBool::new(false));
        self.emit_progress(&guard);
        true
    }

    pub fn snapshot(&self) -> Vec<OpSnapshot> {
        let inner = self.inner.lock().expect("ops lock");
        inner
            .order
            .iter()
            .filter_map(|id| inner.ops.get(id))
            .map(|operation| operation.lock_operation().snapshot())
            .collect()
    }

    /// True while work is active or a cancelled worker is still unwinding.
    pub fn has_unfinished_work(&self) -> bool {
        let inner = self.inner.lock().expect("ops lock");
        inner.ops.values().any(|operation| {
            let operation = operation.lock_operation();
            !operation.is_terminal()
                || (matches!(operation.status, OpStatus::Cancelled)
                    && operation.runtime.completed_at.is_none())
        })
    }

    fn operation(&self, id: &str) -> Option<Arc<Mutex<OpState>>> {
        self.inner.lock().expect("ops lock").ops.get(id).cloned()
    }

    /// The per-op wakeup signal, used to notify a parked worker for `id`.
    fn signal(&self, id: &str) -> Option<Arc<Condvar>> {
        self.inner
            .lock()
            .expect("ops lock")
            .signals
            .get(id)
            .cloned()
    }

    #[cfg(feature = "test-utils")]
    pub fn insert_op_for_tests(&self, state: OpState) {
        let id = state.id.clone();
        let mut inner = self.inner.lock().expect("ops lock");
        inner.ops.insert(id.clone(), Arc::new(Mutex::new(state)));
        inner.signals.insert(id.clone(), Arc::new(Condvar::new()));
        inner.order.push_back(id);
    }

    #[cfg(feature = "test-utils")]
    pub fn run_operation_for_tests(&self, id: &str) {
        let runtime = self.worker_runtime();
        run_operation(id.to_string(), &self.inner, &runtime);
    }

    #[cfg(feature = "test-utils")]
    pub fn release_and_reschedule_for_tests(&self, id: &str) {
        let runtime = self.worker_runtime();
        release_and_reschedule(id, &self.inner, &runtime);
    }
}

pub fn requested_pop(requested: &mut Vec<String>) -> Option<String> {
    if requested.is_empty() {
        return None;
    }
    Some(requested.remove(0))
}

fn reordered_queue(
    order: &VecDeque<String>,
    pending: &HashSet<String>,
    mut requested: Vec<String>,
) -> VecDeque<String> {
    let mut new_order: VecDeque<String> = order
        .iter()
        .filter_map(|id| {
            if pending.contains(id) {
                return requested_pop(&mut requested);
            }
            Some(id.clone())
        })
        .collect();
    let unrequested_pending: Vec<String> = order
        .iter()
        .filter(|id| pending.contains(*id) && !new_order.contains(id))
        .cloned()
        .collect();
    new_order.extend(unrequested_pending);
    new_order
}

impl Inner {
    pub(super) fn new() -> Self {
        Self {
            ops: HashMap::new(),
            signals: HashMap::new(),
            order: VecDeque::new(),
            busy_volumes: HashSet::new(),
            workers: Vec::new(),
        }
    }

    fn pending_ids(&self) -> HashSet<String> {
        self.order
            .iter()
            .filter(|id| self.is_pending(id))
            .cloned()
            .collect()
    }

    fn is_pending(&self, id: &str) -> bool {
        self.ops
            .get(id)
            .is_some_and(|operation| operation.lock_operation().status == OpStatus::Pending)
    }
}
