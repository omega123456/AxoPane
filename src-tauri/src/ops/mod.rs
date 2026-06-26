//! Resource-aware copy/move queue engine.
//!
//! One [`Operation`] is created per user action. The scheduler enforces a single
//! active operation per participating volume (the source *and* destination mount
//! roots), while operations on disjoint volume sets run in parallel. A conflict
//! pauses only the operation that hit it; sibling operations keep running.
//!
//! The engine performs ordinary filesystem copy/move work via [`std::fs`], so it
//! is fully exercisable in integration tests against temp directories without
//! touching any machine-global API.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::volumes::{self, VolumeInfo};

/// How long a completed or cancelled operation lingers in the queue before being auto-removed.
pub const DEFAULT_COMPLETED_RETENTION: Duration = Duration::from_secs(4);
/// How often active operations recompute their short-window instantaneous rate.
pub const DEFAULT_RATE_WINDOW: Duration = Duration::from_millis(250);

/// Number of progress samples required before an ETA is considered stable enough
/// to surface to the UI.
const ETA_STABILIZATION_SAMPLES: u64 = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpKind {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OpStatus {
    Pending,
    Active,
    Paused,
    Conflict,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictResolution {
    Replace,
    Skip,
    Rename,
}

/// A single source item belonging to an operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpItem {
    pub source_path: String,
    pub name: String,
    pub size_bytes: u64,
}

/// Request payload describing one user-initiated copy/move.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOpRequest {
    pub kind: OpKind,
    pub destination_dir: String,
    pub items: Vec<OpItem>,
}

/// A pending conflict awaiting user resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: String,
    pub name: String,
}

/// Per-operation progress snapshot emitted to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpProgress {
    pub operation_id: String,
    pub kind: OpKind,
    pub status: OpStatus,
    pub source_dir: String,
    pub item_names: Vec<String>,
    pub destination_dir: String,
    pub total_items: u64,
    pub completed_items: u64,
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub progress_percent: f64,
    pub bytes_per_second: u64,
    /// Estimated seconds remaining, or `None` until the rate stabilizes.
    pub eta_seconds: Option<u64>,
    pub current_file_name: Option<String>,
    pub current_file_copied_bytes: u64,
    pub current_file_total_bytes: u64,
    pub error_message: Option<String>,
}

/// Snapshot of one operation for queue listing / reorder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpSnapshot {
    pub progress: OpProgress,
    pub conflict: Option<ConflictInfo>,
}

type ProgressEmitter = Arc<dyn Fn(OpProgress) + Send + Sync>;
type ConflictEmitter = Arc<dyn Fn(ConflictInfo) + Send + Sync>;
/// Emits the id of an operation once it has been auto-removed from the queue.
type RemovedEmitter = Arc<dyn Fn(String) + Send + Sync>;
type InstantNow = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Shared, mutable per-operation control + state.
struct OpState {
    id: String,
    kind: OpKind,
    destination_dir: PathBuf,
    items: Vec<OpItem>,
    /// Mount roots this operation touches (source + destination volumes).
    volumes: HashSet<String>,
    status: OpStatus,
    total_items: u64,
    completed_items: u64,
    total_bytes: u64,
    copied_bytes: u64,
    bytes_per_second: u64,
    eta_seconds: Option<u64>,
    sample_count: u64,
    rate_sample_at: Option<Instant>,
    rate_sample_bytes: u64,
    current_file_name: Option<String>,
    current_file_copied: u64,
    current_file_total: u64,
    error_message: Option<String>,
    completed_at: Option<Instant>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    /// Pending conflict, and the channel the worker waits on for resolution.
    conflict: Option<ConflictInfo>,
    conflict_resolution: Option<ConflictResolution>,
    apply_to_all: Option<ConflictResolution>,
    rename_to: Option<String>,
}

impl OpState {
    fn progress(&self) -> OpProgress {
        let visible_copied_bytes = self.visible_copied_bytes();
        let progress_percent = if self.total_bytes == 0 {
            if matches!(self.status, OpStatus::Completed) {
                100.0
            } else {
                0.0
            }
        } else {
            (visible_copied_bytes as f64 / self.total_bytes as f64) * 100.0
        };

        OpProgress {
            operation_id: self.id.clone(),
            kind: self.kind,
            status: self.status,
            source_dir: self
                .items
                .first()
                .map(|item| parent_dir(&item.source_path))
                .unwrap_or_default(),
            item_names: self.items.iter().map(|item| item.name.clone()).collect(),
            destination_dir: self.destination_dir.to_string_lossy().into_owned(),
            total_items: self.total_items,
            completed_items: self.completed_items,
            total_bytes: self.total_bytes,
            copied_bytes: visible_copied_bytes,
            progress_percent,
            bytes_per_second: self.bytes_per_second,
            eta_seconds: self.eta_seconds,
            current_file_name: self.current_file_name.clone(),
            current_file_copied_bytes: self.current_file_copied,
            current_file_total_bytes: self.current_file_total,
            error_message: self.error_message.clone(),
        }
    }

    fn snapshot(&self) -> OpSnapshot {
        OpSnapshot {
            progress: self.progress(),
            conflict: self.conflict.clone(),
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            OpStatus::Completed | OpStatus::Failed | OpStatus::Cancelled
        )
    }

    fn visible_copied_bytes(&self) -> u64 {
        self.copied_bytes.saturating_add(self.current_file_copied)
    }

    fn reset_runtime_state(&mut self) {
        self.completed_items = 0;
        self.copied_bytes = 0;
        self.bytes_per_second = 0;
        self.eta_seconds = None;
        self.sample_count = 0;
        self.rate_sample_at = None;
        self.rate_sample_bytes = 0;
        self.current_file_name = None;
        self.current_file_copied = 0;
        self.current_file_total = 0;
        self.error_message = None;
        self.completed_at = None;
        self.conflict = None;
        self.conflict_resolution = None;
        self.apply_to_all = None;
        self.rename_to = None;
    }
}

struct Inner {
    ops: HashMap<String, Arc<Mutex<OpState>>>,
    /// Submission/scheduling order (also the reorder order for pending items).
    order: VecDeque<String>,
    /// Volumes currently occupied by an active operation.
    busy_volumes: HashSet<String>,
    workers: Vec<JoinHandle<()>>,
}

/// The queue engine. Held as Tauri managed state.
pub struct OpsService {
    inner: Arc<Mutex<Inner>>,
    /// Signalled whenever an operation needs resolution or the scheduler should run.
    resolver: Arc<Condvar>,
    next_id: AtomicU64,
    progress_emitter: Mutex<Option<ProgressEmitter>>,
    conflict_emitter: Mutex<Option<ConflictEmitter>>,
    removed_emitter: Mutex<Option<RemovedEmitter>>,
    completed_retention: Duration,
    rate_window: Duration,
    instant_now: InstantNow,
    volumes: Mutex<Vec<VolumeInfo>>,
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
        Self {
            inner: Arc::new(Mutex::new(Inner {
                ops: HashMap::new(),
                order: VecDeque::new(),
                busy_volumes: HashSet::new(),
                workers: Vec::new(),
            })),
            resolver: Arc::new(Condvar::new()),
            next_id: AtomicU64::new(1),
            progress_emitter: Mutex::new(None),
            conflict_emitter: Mutex::new(None),
            removed_emitter: Mutex::new(None),
            completed_retention,
            rate_window,
            instant_now: Arc::new(Instant::now),
            volumes: Mutex::new(volumes::list_volumes()),
        }
    }

    /// Override the volume table used for identity (tests inject deterministic data).
    pub fn set_volumes(&self, volumes: Vec<VolumeInfo>) {
        *self.volumes.lock().expect("volumes lock") = volumes;
    }

    pub fn set_progress_emitter<F>(&self, emitter: F)
    where
        F: Fn(OpProgress) + Send + Sync + 'static,
    {
        *self.progress_emitter.lock().expect("progress emitter lock") = Some(Arc::new(emitter));
    }

    pub fn set_conflict_emitter<F>(&self, emitter: F)
    where
        F: Fn(ConflictInfo) + Send + Sync + 'static,
    {
        *self.conflict_emitter.lock().expect("conflict emitter lock") = Some(Arc::new(emitter));
    }

    pub fn set_removed_emitter<F>(&self, emitter: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        *self.removed_emitter.lock().expect("removed emitter lock") = Some(Arc::new(emitter));
    }

    #[cfg(feature = "test-utils")]
    pub fn set_instant_now_for_tests<F>(&mut self, instant_now: F)
    where
        F: Fn() -> Instant + Send + Sync + 'static,
    {
        self.instant_now = Arc::new(instant_now);
    }

    fn schedule_auto_remove_for(&self, id: &str) {
        let removed = self
            .removed_emitter
            .lock()
            .expect("removed emitter lock")
            .clone();
        schedule_auto_remove(id, &self.inner, removed, self.completed_retention);
    }

    fn emit_progress(&self, op: &OpState) {
        if let Some(emitter) = self
            .progress_emitter
            .lock()
            .expect("progress emitter lock")
            .clone()
        {
            emitter(op.progress());
        }
    }

    fn volumes_for(&self, paths: &[&Path]) -> HashSet<String> {
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
        source_paths.push(destination_dir.clone());
        let path_refs: Vec<&Path> = source_paths.iter().map(PathBuf::as_path).collect();
        let op_volumes = self.volumes_for(&path_refs);

        let total_bytes: u64 = request.items.iter().map(|item| item.size_bytes).sum();
        let total_items = request.items.len() as u64;

        let state = OpState {
            id: id.clone(),
            kind: request.kind,
            destination_dir,
            items: request.items,
            volumes: op_volumes,
            status: OpStatus::Pending,
            total_items,
            completed_items: 0,
            total_bytes,
            copied_bytes: 0,
            bytes_per_second: 0,
            eta_seconds: None,
            sample_count: 0,
            rate_sample_at: None,
            rate_sample_bytes: 0,
            current_file_name: None,
            current_file_copied: 0,
            current_file_total: 0,
            error_message: None,
            completed_at: None,
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            conflict: None,
            conflict_resolution: None,
            apply_to_all: None,
            rename_to: None,
        };

        {
            let mut inner = self.inner.lock().expect("ops lock");
            let op = Arc::new(Mutex::new(state));
            inner.ops.insert(id.clone(), op.clone());
            inner.order.push_back(id.clone());
            self.emit_progress(&op.lock().expect("op lock"));
        }

        self.schedule();
        id
    }

    /// Try to dispatch pending operations onto free volumes.
    fn schedule(self: &OpsService) {
        // Collect ids to start while holding the inner lock, but spawn outside it.
        let mut to_start: Vec<String> = Vec::new();

        {
            let mut inner = self.inner.lock().expect("ops lock");
            let order: Vec<String> = inner.order.iter().cloned().collect();

            for id in order {
                let Some(op_arc) = inner.ops.get(&id).cloned() else {
                    continue;
                };
                let op = op_arc.lock().expect("op lock");
                if op.status != OpStatus::Pending {
                    continue;
                }
                if op
                    .volumes
                    .iter()
                    .any(|vol| inner.busy_volumes.contains(vol))
                {
                    continue;
                }
                let volumes = op.volumes.clone();
                drop(op);
                for vol in &volumes {
                    inner.busy_volumes.insert(vol.clone());
                }
                to_start.push(id);
            }
        }

        for id in to_start {
            self.spawn_worker(id);
        }
    }

    fn spawn_worker(self: &OpsService, id: String) {
        let inner = self.inner.clone();
        let resolver = self.resolver.clone();
        let progress = self
            .progress_emitter
            .lock()
            .expect("progress emitter lock")
            .clone();
        let conflict = self
            .conflict_emitter
            .lock()
            .expect("conflict emitter lock")
            .clone();
        let removed = self
            .removed_emitter
            .lock()
            .expect("removed emitter lock")
            .clone();
        let retention = self.completed_retention;
        let rate_window = self.rate_window;
        let instant_now = self.instant_now.clone();

        let handle = thread::spawn(move || {
            run_operation(
                id.clone(),
                &inner,
                &resolver,
                progress.clone(),
                conflict.clone(),
                rate_window,
                instant_now.clone(),
            );
            release_and_reschedule(
                &id,
                &inner,
                &resolver,
                progress,
                conflict,
                removed,
                retention,
                rate_window,
                instant_now,
            );
        });

        let mut inner = self.inner.lock().expect("ops lock");
        inner.workers.retain(|worker| !worker.is_finished());
        inner.workers.push(handle);
    }

    pub fn pause_op(&self, id: &str) {
        if let Some(op) = self.op(id) {
            let mut guard = op.lock().expect("op lock");
            if matches!(guard.status, OpStatus::Active) {
                guard.pause.store(true, Ordering::Relaxed);
                guard.status = OpStatus::Paused;
                self.emit_progress(&guard);
            }
        }
    }

    pub fn resume_op(&self, id: &str) {
        let resumed = if let Some(op) = self.op(id) {
            let mut guard = op.lock().expect("op lock");
            if matches!(guard.status, OpStatus::Paused) {
                guard.pause.store(false, Ordering::Relaxed);
                guard.status = OpStatus::Active;
                self.emit_progress(&guard);
                true
            } else {
                false
            }
        } else {
            false
        };

        if resumed {
            self.resolver.notify_all();
        }
    }

    pub fn cancel_op(&self, id: &str) {
        let (notify, schedule_removal) = if let Some(op) = self.op(id) {
            let mut guard = op.lock().expect("op lock");
            if guard.is_terminal() {
                (false, false)
            } else if matches!(guard.status, OpStatus::Pending) {
                // Never started: mark cancelled immediately.
                guard.status = OpStatus::Cancelled;
                guard.completed_at = Some(Instant::now());
                self.emit_progress(&guard);
                (false, true)
            } else {
                guard.cancel.store(true, Ordering::Relaxed);
                guard.pause.store(false, Ordering::Relaxed);
                (true, false)
            }
        } else {
            (false, false)
        };

        if notify {
            self.resolver.notify_all();
        }
        if schedule_removal {
            self.schedule_auto_remove_for(id);
        }
    }

    /// Reorder *pending* operations. Unknown / non-pending ids are ignored; any
    /// pending ids not named keep their relative order at the back.
    pub fn reorder_ops(&self, ids: &[String]) {
        let mut inner = self.inner.lock().expect("ops lock");
        let pending: HashSet<String> = inner
            .order
            .iter()
            .filter(|id| {
                inner
                    .ops
                    .get(*id)
                    .map(|op| op.lock().expect("op lock").status == OpStatus::Pending)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        let mut new_order: VecDeque<String> = VecDeque::new();
        // Non-pending ops keep their absolute position relative to each other.
        let mut requested: Vec<String> = ids
            .iter()
            .filter(|id| pending.contains(*id))
            .cloned()
            .collect();
        for id in inner.order.iter() {
            if pending.contains(id) {
                if let Some(next) = requested_pop(&mut requested) {
                    new_order.push_back(next);
                }
            } else {
                new_order.push_back(id.clone());
            }
        }
        // Append any pending ids that were not part of the request.
        for id in inner.order.iter() {
            if pending.contains(id) && !new_order.contains(id) {
                new_order.push_back(id.clone());
            }
        }
        inner.order = new_order;
    }

    pub fn resolve_conflict(
        &self,
        id: &str,
        resolution: ConflictResolution,
        apply_to_all: bool,
        rename_to: Option<String>,
    ) {
        let notify = if let Some(op) = self.op(id) {
            let mut guard = op.lock().expect("op lock");
            if matches!(guard.status, OpStatus::Conflict) {
                guard.conflict_resolution = Some(resolution);
                guard.rename_to = rename_to;
                if apply_to_all {
                    guard.apply_to_all = Some(resolution);
                }
                guard.conflict = None;
                guard.status = OpStatus::Active;
                self.emit_progress(&guard);
                true
            } else {
                false
            }
        } else {
            false
        };

        if notify {
            self.resolver.notify_all();
        }
    }

    /// Retry a failed operation by re-queuing it.
    pub fn retry_op(&self, id: &str) {
        let requeue = if let Some(op) = self.op(id) {
            let mut guard = op.lock().expect("op lock");
            if matches!(guard.status, OpStatus::Failed) {
                guard.status = OpStatus::Pending;
                guard.reset_runtime_state();
                guard.cancel = Arc::new(AtomicBool::new(false));
                guard.pause = Arc::new(AtomicBool::new(false));
                self.emit_progress(&guard);
                true
            } else {
                false
            }
        } else {
            false
        };

        if requeue {
            self.schedule();
        }
    }

    pub fn snapshot(&self) -> Vec<OpSnapshot> {
        let inner = self.inner.lock().expect("ops lock");
        inner
            .order
            .iter()
            .filter_map(|id| inner.ops.get(id))
            .map(|op| op.lock().expect("op lock").snapshot())
            .collect()
    }

    /// True when any operation is still active, pending, paused or in conflict.
    pub fn has_unfinished_work(&self) -> bool {
        let inner = self.inner.lock().expect("ops lock");
        inner
            .ops
            .values()
            .any(|op| !op.lock().expect("op lock").is_terminal())
    }

    fn op(&self, id: &str) -> Option<Arc<Mutex<OpState>>> {
        self.inner.lock().expect("ops lock").ops.get(id).cloned()
    }
}

fn requested_pop(requested: &mut Vec<String>) -> Option<String> {
    if requested.is_empty() {
        None
    } else {
        Some(requested.remove(0))
    }
}

/// Run a single operation to completion / failure / cancel. Blocks the worker
/// thread; conflicts park here on the condvar until resolved.
fn run_operation(
    id: String,
    inner: &Arc<Mutex<Inner>>,
    resolver: &Arc<Condvar>,
    progress: Option<ProgressEmitter>,
    conflict: Option<ConflictEmitter>,
    rate_window: Duration,
    instant_now: InstantNow,
) {
    let op_arc = {
        let guard = inner.lock().expect("ops lock");
        guard.ops.get(&id).cloned()
    };
    let Some(op_arc) = op_arc else {
        return;
    };

    {
        let mut op = op_arc.lock().expect("op lock");
        op.status = OpStatus::Active;
        op.rate_sample_at = Some(instant_now());
        op.rate_sample_bytes = 0;
        if let Some(emitter) = &progress {
            emitter(op.progress());
        }
    }

    // Directory items arrive with size 0 from the frontend; measure their real
    // byte totals on the worker thread so progress tracks bytes copied rather
    // than whole items (a single folder otherwise jumps straight from 0% to
    // 100%). Files keep their frontend-provided size.
    let cancel = op_arc.lock().expect("op lock").cancel.clone();
    let mut items = op_arc.lock().expect("op lock").items.clone();
    let mut total_bytes: u64 = 0;
    for item in &mut items {
        if item.size_bytes == 0 {
            let path = Path::new(&item.source_path);
            if path.is_dir() {
                item.size_bytes = measure_tree_size(path, &cancel);
            }
        }
        total_bytes = total_bytes.saturating_add(item.size_bytes);
    }
    {
        let mut op = op_arc.lock().expect("op lock");
        op.items = items.clone();
        op.total_bytes = total_bytes;
        if let Some(emitter) = &progress {
            emitter(op.progress());
        }
    }

    let start = instant_now();

    for item in items {
        {
            let op = op_arc.lock().expect("op lock");
            if op.cancel.load(Ordering::Relaxed) || op.error_message.is_some() {
                break;
            }
        }

        let ctx = WorkerCtx {
            op_arc: &op_arc,
            resolver,
            progress: &progress,
            start,
            rate_window,
            instant_now: &instant_now,
        };
        process_item(&ctx, &item, &conflict);

        {
            let op = op_arc.lock().expect("op lock");
            if op.cancel.load(Ordering::Relaxed) || op.error_message.is_some() {
                break;
            }
        }
    }

    let mut op = op_arc.lock().expect("op lock");
    if op.cancel.load(Ordering::Relaxed) {
        op.status = OpStatus::Cancelled;
    } else if op.error_message.is_some() {
        op.status = OpStatus::Failed;
    } else {
        op.status = OpStatus::Completed;
        op.copied_bytes = op.total_bytes;
    }
    op.completed_at = Some(Instant::now());
    op.current_file_name = None;
    op.current_file_copied = 0;
    op.current_file_total = 0;
    if let Some(emitter) = &progress {
        emitter(op.progress());
    }
}

/// Shared context threaded through the copy/move worker helpers. Bundling these
/// fields keeps the recursive copy signatures small (no `too_many_arguments`).
struct WorkerCtx<'a> {
    op_arc: &'a Arc<Mutex<OpState>>,
    resolver: &'a Arc<Condvar>,
    progress: &'a Option<ProgressEmitter>,
    start: Instant,
    rate_window: Duration,
    instant_now: &'a InstantNow,
}

fn process_item(ctx: &WorkerCtx<'_>, item: &OpItem, conflict: &Option<ConflictEmitter>) {
    let op_arc = ctx.op_arc;
    let (kind, destination_dir) = {
        let op = op_arc.lock().expect("op lock");
        (op.kind, op.destination_dir.clone())
    };

    let source = PathBuf::from(&item.source_path);
    let mut target = destination_dir.join(&item.name);

    if source.is_dir() && is_nested_copy_target(&source, &target) {
        let action = match kind {
            OpKind::Copy => "copy",
            OpKind::Move => "move",
        };
        let mut op = op_arc.lock().expect("op lock");
        op.error_message = Some(format!(
            "Cannot {action} \"{}\" into one of its own descendants.",
            source.display()
        ));
        return;
    }

    // Conflict detection + resolution.
    if target.exists() {
        let resolution = resolve_or_park(ctx, item, &target, conflict);
        match resolution {
            Some((ConflictResolution::Skip, _)) => {
                advance_item(ctx, item);
                return;
            }
            Some((ConflictResolution::Replace, _)) => {
                let _ = remove_target(&target);
            }
            Some((ConflictResolution::Rename, rename_to)) => {
                let new_name =
                    rename_to.unwrap_or_else(|| unique_name(&destination_dir, &item.name));
                target = destination_dir.join(new_name);
            }
            None => {
                // Cancelled while parked.
                return;
            }
        }
    }

    // Wait out any pause before doing IO.
    wait_while_paused(op_arc, ctx.resolver);
    if op_arc
        .lock()
        .expect("op lock")
        .cancel
        .load(Ordering::Relaxed)
    {
        return;
    }

    {
        let mut op = op_arc.lock().expect("op lock");
        op.current_file_name = Some(item.name.clone());
        op.current_file_copied = 0;
        op.current_file_total = item.size_bytes;
        if let Some(emitter) = ctx.progress {
            emitter(op.progress());
        }
    }

    let result = if matches!(kind, OpKind::Move) {
        move_path(&source, &target, ctx)
    } else {
        copy_path(&source, &target, ctx)
    };

    match result {
        Ok(()) => {
            if op_arc
                .lock()
                .expect("op lock")
                .cancel
                .load(Ordering::Relaxed)
            {
                return;
            }
            advance_item(ctx, item)
        }
        Err(message) => {
            let mut op = op_arc.lock().expect("op lock");
            op.error_message = Some(message);
        }
    }
}

/// Surface a conflict and block until the user resolves it (or it is cancelled).
fn resolve_or_park(
    ctx: &WorkerCtx<'_>,
    item: &OpItem,
    target: &Path,
    conflict: &Option<ConflictEmitter>,
) -> Option<(ConflictResolution, Option<String>)> {
    let op_arc = ctx.op_arc;
    {
        let op = op_arc.lock().expect("op lock");
        if let Some(blanket) = op.apply_to_all {
            return Some((blanket, None));
        }
    }

    let info = ConflictInfo {
        operation_id: op_arc.lock().expect("op lock").id.clone(),
        source_path: item.source_path.clone(),
        destination_path: target.to_string_lossy().into_owned(),
        name: item.name.clone(),
    };

    {
        let mut op = op_arc.lock().expect("op lock");
        op.status = OpStatus::Conflict;
        op.conflict = Some(info.clone());
        op.conflict_resolution = None;
        if let Some(emitter) = ctx.progress {
            emitter(op.progress());
        }
    }
    if let Some(emitter) = conflict {
        emitter(info);
    }

    let mut guard = op_arc.lock().expect("op lock");
    loop {
        if guard.cancel.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(resolution) = guard.conflict_resolution.take() {
            let rename = guard.rename_to.take();
            return Some((resolution, rename));
        }
        guard = ctx.resolver.wait(guard).expect("conflict wait");
    }
}

fn wait_while_paused(op_arc: &Arc<Mutex<OpState>>, resolver: &Arc<Condvar>) {
    let mut guard = op_arc.lock().expect("op lock");
    while guard.pause.load(Ordering::Relaxed) && !guard.cancel.load(Ordering::Relaxed) {
        guard = resolver.wait(guard).expect("pause wait");
    }
}

fn advance_item(ctx: &WorkerCtx<'_>, item: &OpItem) {
    let mut op = ctx.op_arc.lock().expect("op lock");
    op.completed_items += 1;
    op.copied_bytes = op.copied_bytes.saturating_add(item.size_bytes);
    op.current_file_name = None;
    op.current_file_copied = 0;
    op.current_file_total = 0;
    refresh_rate_locked_with_now(&mut op, ctx.start, ctx.rate_window, (ctx.instant_now)());

    if let Some(emitter) = ctx.progress {
        emitter(op.progress());
    }
}

fn report_chunk_progress(ctx: &WorkerCtx<'_>, copied_delta: u64) {
    let mut op = ctx.op_arc.lock().expect("op lock");
    op.current_file_copied = op.current_file_copied.saturating_add(copied_delta);
    refresh_rate_locked_with_now(&mut op, ctx.start, ctx.rate_window, (ctx.instant_now)());
    if let Some(emitter) = ctx.progress {
        emitter(op.progress());
    }
}

fn refresh_rate_locked_with_now(
    op: &mut OpState,
    start: Instant,
    rate_window: Duration,
    now: Instant,
) {
    op.sample_count += 1;
    let visible_copied = op.visible_copied_bytes();
    let last_sample_at = op.rate_sample_at.unwrap_or(start);
    let last_sample_bytes = op.rate_sample_bytes;
    let should_recompute = rate_window.is_zero()
        || op.bytes_per_second == 0
        || now.duration_since(last_sample_at) >= rate_window;

    if should_recompute {
        let sample_elapsed = now.duration_since(last_sample_at).as_secs_f64().max(0.001);
        let sample_bytes = visible_copied.saturating_sub(last_sample_bytes);
        op.bytes_per_second = (sample_bytes as f64 / sample_elapsed) as u64;
        op.rate_sample_at = Some(now);
        op.rate_sample_bytes = visible_copied;
    }

    let stable_elapsed = start.elapsed().as_secs_f64().max(0.001);
    let stable_rate = (visible_copied as f64 / stable_elapsed) as u64;
    if op.sample_count >= ETA_STABILIZATION_SAMPLES && stable_rate > 0 {
        let remaining = op.total_bytes.saturating_sub(visible_copied);
        op.eta_seconds = Some(remaining / stable_rate.max(1));
    }
}

/// Once a worker finishes, free its volumes and try to schedule waiting work.
fn release_and_reschedule(
    id: &str,
    inner: &Arc<Mutex<Inner>>,
    resolver: &Arc<Condvar>,
    progress: Option<ProgressEmitter>,
    conflict: Option<ConflictEmitter>,
    removed: Option<RemovedEmitter>,
    retention: Duration,
    rate_window: Duration,
    instant_now: InstantNow,
) {
    let volumes = {
        let guard = inner.lock().expect("ops lock");
        guard
            .ops
            .get(id)
            .map(|op| op.lock().expect("op lock").volumes.clone())
            .unwrap_or_default()
    };

    {
        let mut guard = inner.lock().expect("ops lock");
        for vol in &volumes {
            guard.busy_volumes.remove(vol);
        }
    }

    // Dispatch any newly-eligible pending operations.
    let to_start = collect_startable(inner);
    for next in to_start {
        let inner = inner.clone();
        let resolver = resolver.clone();
        let progress = progress.clone();
        let conflict = conflict.clone();
        let removed = removed.clone();
        let instant_now = instant_now.clone();
        thread::spawn(move || {
            run_operation(
                next.clone(),
                &inner,
                &resolver,
                progress.clone(),
                conflict.clone(),
                rate_window,
                instant_now.clone(),
            );
            release_and_reschedule(
                &next,
                &inner,
                &resolver,
                progress,
                conflict,
                removed,
                retention,
                rate_window,
                instant_now,
            );
        });
    }

    // Auto-remove completed/cancelled operations after the retention window.
    schedule_auto_remove(id, inner, removed, retention);
}

fn collect_startable(inner: &Arc<Mutex<Inner>>) -> Vec<String> {
    let mut guard = inner.lock().expect("ops lock");
    let order: Vec<String> = guard.order.iter().cloned().collect();
    let mut to_start = Vec::new();

    for id in order {
        let Some(op_arc) = guard.ops.get(&id).cloned() else {
            continue;
        };
        let op = op_arc.lock().expect("op lock");
        if op.status != OpStatus::Pending {
            continue;
        }
        if op
            .volumes
            .iter()
            .any(|vol| guard.busy_volumes.contains(vol))
        {
            continue;
        }
        let volumes = op.volumes.clone();
        drop(op);
        for vol in &volumes {
            guard.busy_volumes.insert(vol.clone());
        }
        to_start.push(id);
    }

    to_start
}

fn schedule_auto_remove(
    id: &str,
    inner: &Arc<Mutex<Inner>>,
    removed: Option<RemovedEmitter>,
    retention: Duration,
) {
    let should_remove = inner
        .lock()
        .expect("ops lock")
        .ops
        .get(id)
        .map(|op| {
            matches!(
                op.lock().expect("op lock").status,
                OpStatus::Completed | OpStatus::Cancelled
            )
        })
        .unwrap_or(false);

    if !should_remove {
        return;
    }

    let inner = inner.clone();
    let id = id.to_string();
    thread::spawn(move || {
        thread::sleep(retention);
        let mut guard = inner.lock().expect("ops lock");
        let still_terminal = guard
            .ops
            .get(&id)
            .map(|op| {
                matches!(
                    op.lock().expect("op lock").status,
                    OpStatus::Completed | OpStatus::Cancelled
                )
            })
            .unwrap_or(false);
        if still_terminal {
            guard.ops.remove(&id);
            guard.order.retain(|entry| entry != &id);
            drop(guard);
            // Tell the UI to prune the card now that the backend forgot the op.
            if let Some(emitter) = &removed {
                emitter(id);
            }
        }
    });
}

// --- Filesystem helpers --------------------------------------------------------

/// Sum the byte size of a directory tree to drive byte-accurate progress.
/// Symlinks are not followed (consistent with the copy's link guard, and to stay
/// cycle-safe) and IO errors are skipped so measurement never aborts the op.
fn measure_tree_size(path: &Path, cancel: &Arc<AtomicBool>) -> u64 {
    let mut total = 0_u64;
    let Ok(entries) = fs::read_dir(path) else {
        return total;
    };
    for entry in entries.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            total = total.saturating_add(measure_tree_size(&entry.path(), cancel));
        } else if file_type.is_file() {
            if let Ok(metadata) = entry.metadata() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

fn copy_path(source: &Path, target: &Path, ctx: &WorkerCtx<'_>) -> Result<(), String> {
    if source.is_dir() {
        let root = source.canonicalize().map_err(|error| error.to_string())?;
        let mut visited = HashSet::from([root.clone()]);
        copy_dir_recursive(source, target, &root, &mut visited, ctx)
    } else {
        copy_file_with_progress(source, target, ctx)
    }
}

fn copy_dir_recursive(
    source: &Path,
    target: &Path,
    root: &Path,
    visited: &mut HashSet<PathBuf>,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        if ctx
            .op_arc
            .lock()
            .expect("op lock")
            .cancel
            .load(Ordering::Relaxed)
        {
            return Ok(());
        }

        let entry = entry.map_err(|error| error.to_string())?;
        let child_target = target.join(entry.file_name());
        if entry.path().is_dir() {
            let canonical_child = entry
                .path()
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !canonical_child.starts_with(root) {
                return Err(format!(
                    "Refusing to copy linked directory \"{}\" outside the selected source tree.",
                    entry.path().display()
                ));
            }
            if !visited.insert(canonical_child.clone()) {
                return Err(format!(
                    "Refusing to recurse into cyclic directory link \"{}\".",
                    entry.path().display()
                ));
            }
            copy_dir_recursive(&entry.path(), &child_target, root, visited, ctx)?;
            visited.remove(&canonical_child);
        } else {
            copy_file_with_progress(&entry.path(), &child_target, ctx)?;
        }
    }
    Ok(())
}

fn move_path(source: &Path, target: &Path, ctx: &WorkerCtx<'_>) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-volume move: copy then delete the source.
            copy_path(source, target, ctx)?;
            if ctx
                .op_arc
                .lock()
                .expect("op lock")
                .cancel
                .load(Ordering::Relaxed)
            {
                return Ok(());
            }
            remove_source(source)
        }
    }
}

fn copy_file_with_progress(
    source: &Path,
    target: &Path,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut reader = fs::File::open(source).map_err(|error| error.to_string())?;
    let mut writer = fs::File::create(target).map_err(|error| error.to_string())?;
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        wait_while_paused(ctx.op_arc, ctx.resolver);
        if ctx
            .op_arc
            .lock()
            .expect("op lock")
            .cancel
            .load(Ordering::Relaxed)
        {
            return Ok(());
        }

        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }

        writer
            .write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        report_chunk_progress(ctx, read as u64);
    }

    writer.flush().map_err(|error| error.to_string())
}

fn remove_target(target: &Path) -> Result<(), String> {
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(|error| error.to_string())
    } else {
        fs::remove_file(target).map_err(|error| error.to_string())
    }
}

fn remove_source(source: &Path) -> Result<(), String> {
    if source.is_dir() {
        fs::remove_dir_all(source).map_err(|error| error.to_string())
    } else {
        fs::remove_file(source).map_err(|error| error.to_string())
    }
}

fn unique_name(dir: &Path, name: &str) -> String {
    let (stem, ext) = split_name(name);
    let mut counter = 1;
    loop {
        let candidate = if ext.is_empty() {
            format!("{stem} ({counter})")
        } else {
            format!("{stem} ({counter}).{ext}")
        };
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        counter += 1;
    }
}

fn split_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(index) if index > 0 => (name[..index].to_string(), name[index + 1..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}

fn parent_dir(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Resolve which mount root a path belongs to, falling back to the longest-prefix
/// match against the known volume table.
pub fn volume_root_for(path: &Path, volumes: &[VolumeInfo]) -> String {
    let path_text = path.to_string_lossy().to_ascii_lowercase();

    let matched = volumes
        .iter()
        .filter(|volume| {
            let mount = volume.mount_root.to_ascii_lowercase();
            path_text == mount
                || path_text.strip_prefix(&mount).is_some_and(|remainder| {
                    remainder.starts_with('\\') || remainder.starts_with('/')
                })
        })
        .max_by_key(|volume| volume.mount_root.len());

    if let Some(volume) = matched {
        return volume.mount_root.to_ascii_lowercase();
    }

    // Fallback: derive a root from the path itself so disjoint test paths still
    // serialize sensibly (drive letter on Windows, first component otherwise).
    fallback_root(&path_text)
}

fn fallback_root(path_text: &str) -> String {
    if let Some(index) = path_text.find(':') {
        return path_text[..=index].to_string();
    }
    let trimmed = path_text.trim_start_matches('/');
    match trimmed.split('/').next() {
        Some(first) if !first.is_empty() => format!("/{first}"),
        _ => "/".to_string(),
    }
}

fn is_nested_copy_target(source: &Path, target: &Path) -> bool {
    let source_components = normalized_components(source);
    let target_components = normalized_components(target);

    target_components.len() > source_components.len()
        && target_components.starts_with(&source_components)
}

fn normalized_components(path: &Path) -> Vec<String> {
    use std::path::Component;

    let mut normalized = Vec::new();
    for component in dunce::simplified(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(normalized.last(), Some(last) if last != "..") {
                    normalized.pop();
                } else {
                    normalized.push("..".to_string());
                }
            }
            Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str().to_string_lossy().to_ascii_lowercase());
            }
            Component::Normal(value) => {
                #[cfg(windows)]
                {
                    normalized.push(value.to_string_lossy().to_ascii_lowercase());
                }
                #[cfg(not(windows))]
                {
                    normalized.push(value.to_string_lossy().into_owned());
                }
            }
        }
    }
    normalized
}
