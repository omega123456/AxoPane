// Resource-aware copy/move queue engine.
//
// One operation is created per user action. The scheduler enforces a single
// active operation per participating volume (the source and destination mount
// roots), while operations on disjoint volume sets run in parallel. A conflict
// pauses only the operation that hit it; sibling operations keep running.
//
// The engine performs ordinary filesystem copy/move work via `std::fs`, so it
// is fully exercisable in integration tests against temp directories without
// touching any machine-global API.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::volumes::VolumeInfo;

/// How long a completed or cancelled operation lingers in the queue before being auto-removed.
pub const DEFAULT_COMPLETED_RETENTION: Duration = Duration::from_secs(4);
/// How often active operations recompute their short-window instantaneous rate.
pub const DEFAULT_RATE_WINDOW: Duration = Duration::from_millis(250);
/// Minimum spacing between progress emissions while deleting individual files.
const DELETE_EMIT_INTERVAL: Duration = Duration::from_millis(90);

/// Number of progress samples required before an ETA is considered stable enough
/// to surface to the UI.
const ETA_STABILIZATION_SAMPLES: u64 = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpKind {
    Copy,
    Move,
    Delete,
    Compress,
    Extract,
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

struct DeleteThrottle {
    last_emit: Instant,
    interval: Duration,
}

impl DeleteThrottle {
    fn new(last_emit: Instant, interval: Duration) -> Self {
        Self {
            last_emit,
            interval,
        }
    }
}

/// Shared, mutable per-operation control + state.
pub struct OpState {
    pub id: String,
    pub kind: OpKind,
    pub destination_dir: PathBuf,
    pub items: Vec<OpItem>,
    /// Mount roots this operation touches (source + destination volumes).
    pub volumes: HashSet<String>,
    pub status: OpStatus,
    pub total_items: u64,
    pub completed_items: u64,
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub bytes_per_second: u64,
    pub eta_seconds: Option<u64>,
    pub sample_count: u64,
    pub rate_sample_at: Option<Instant>,
    pub rate_sample_bytes: u64,
    pub current_file_name: Option<String>,
    pub current_file_copied: u64,
    pub current_file_total: u64,
    pub error_message: Option<String>,
    pub completed_at: Option<Instant>,
    pub cancel: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    /// Pending conflict, and the channel the worker waits on for resolution.
    pub conflict: Option<ConflictInfo>,
    pub conflict_resolution: Option<ConflictResolution>,
    pub apply_to_all: Option<ConflictResolution>,
    pub rename_to: Option<String>,
}

impl OpState {
    pub fn progress(&self) -> OpProgress {
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

    pub fn snapshot(&self) -> OpSnapshot {
        OpSnapshot {
            progress: self.progress(),
            conflict: self.conflict.clone(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            OpStatus::Completed | OpStatus::Failed | OpStatus::Cancelled
        )
    }

    pub fn visible_copied_bytes(&self) -> u64 {
        self.copied_bytes.saturating_add(self.current_file_copied)
    }

    pub fn reset_runtime_state(&mut self) {
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
    /// Per-operation wakeup signal. Each op owns its own `Condvar` so that a
    /// parked worker always pairs the same `Condvar` with the same op mutex —
    /// `Condvar` panics on platforms (e.g. macOS pthread) if a single instance
    /// is waited on with two different mutexes, which happened when one shared
    /// `Condvar` served every op's per-op `OpState` mutex.
    signals: HashMap<String, Arc<Condvar>>,
    /// Submission/scheduling order (also the reorder order for pending items).
    order: VecDeque<String>,
    /// Volumes currently occupied by an active operation.
    busy_volumes: HashSet<String>,
    workers: Vec<JoinHandle<()>>,
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
                signals: HashMap::new(),
                order: VecDeque::new(),
                busy_volumes: HashSet::new(),
                workers: Vec::new(),
            })),
            next_id: AtomicU64::new(1),
            progress_emitter: Mutex::new(None),
            conflict_emitter: Mutex::new(None),
            removed_emitter: Mutex::new(None),
            completed_retention,
            rate_window,
            instant_now: Arc::new(Instant::now),
            volumes: Mutex::new(crate::volumes::list_volumes()),
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
        schedule_auto_remove(id, &self.inner, removed, self.completed_retention);
    }

    pub fn emit_progress(&self, op: &OpState) {
        if let Some(emitter) = self
            .progress_emitter
            .lock()
            .expect("progress emitter lock")
            .clone()
        {
            emitter(op.progress());
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
            inner.signals.insert(id.clone(), Arc::new(Condvar::new()));
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
        let runtime = WorkerRuntime {
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
        };

        let handle = thread::spawn(move || {
            run_operation(id.clone(), &inner, &runtime);
            release_and_reschedule(&id, &inner, &runtime);
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
            if let Some(signal) = self.signal(id) {
                signal.notify_all();
            }
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
            if let Some(signal) = self.signal(id) {
                signal.notify_all();
            }
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
            if let Some(signal) = self.signal(id) {
                signal.notify_all();
            }
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
        let runtime = WorkerRuntime {
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
        };
        run_operation(id.to_string(), &self.inner, &runtime);
    }

    #[cfg(feature = "test-utils")]
    pub fn release_and_reschedule_for_tests(&self, id: &str) {
        let runtime = WorkerRuntime {
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
        };
        release_and_reschedule(id, &self.inner, &runtime);
    }
}

pub fn requested_pop(requested: &mut Vec<String>) -> Option<String> {
    if requested.is_empty() {
        None
    } else {
        Some(requested.remove(0))
    }
}

/// Cloneable bundle of the emitters and timing config a worker thread needs.
/// Bundling keeps `run_operation` / `release_and_reschedule` signatures small
/// (no `too_many_arguments`), mirroring how [`WorkerCtx`] bundles per-item state.
#[derive(Clone)]
struct WorkerRuntime {
    progress: Option<ProgressEmitter>,
    conflict: Option<ConflictEmitter>,
    removed: Option<RemovedEmitter>,
    retention: Duration,
    rate_window: Duration,
    instant_now: InstantNow,
}

/// Run a single operation to completion / failure / cancel. Blocks the worker
/// thread; conflicts park here on the condvar until resolved.
fn run_operation(id: String, inner: &Arc<Mutex<Inner>>, runtime: &WorkerRuntime) {
    let progress = runtime.progress.clone();
    let conflict = runtime.conflict.clone();
    let rate_window = runtime.rate_window;
    let instant_now = runtime.instant_now.clone();

    let (op_arc, signal) = {
        let guard = inner.lock().expect("ops lock");
        (guard.ops.get(&id).cloned(), guard.signals.get(&id).cloned())
    };
    let (Some(op_arc), Some(signal)) = (op_arc, signal) else {
        return;
    };
    let resolver = &signal;

    {
        let mut op = op_arc.lock().expect("op lock");
        op.status = OpStatus::Active;
        op.rate_sample_at = Some(instant_now());
        op.rate_sample_bytes = 0;
        if let Some(emitter) = &progress {
            emitter(op.progress());
        }
    }

    let items = op_arc.lock().expect("op lock").items.clone();
    measure_delete_totals_up_front(&items, &op_arc, &progress);

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
pub struct WorkerCtx<'a> {
    pub op_arc: &'a Arc<Mutex<OpState>>,
    pub resolver: &'a Arc<Condvar>,
    pub progress: &'a Option<ProgressEmitter>,
    pub start: Instant,
    pub rate_window: Duration,
    pub instant_now: &'a InstantNow,
}

fn process_item(ctx: &WorkerCtx<'_>, item: &OpItem, conflict: &Option<ConflictEmitter>) {
    let op_arc = ctx.op_arc;
    let (kind, destination_dir) = {
        let op = op_arc.lock().expect("op lock");
        (op.kind, op.destination_dir.clone())
    };

    let source = PathBuf::from(&item.source_path);

    // Queue-native operations that do not use copy/move conflict handling.
    if matches!(kind, OpKind::Delete) {
        wait_while_paused(op_arc, ctx.resolver);
        if op_arc
            .lock()
            .expect("op lock")
            .cancel
            .load(Ordering::Relaxed)
        {
            return;
        }
        match delete_path_with_progress(&source, false, ctx) {
            Ok(()) => {
                if op_arc
                    .lock()
                    .expect("op lock")
                    .cancel
                    .load(Ordering::Relaxed)
                {
                    return;
                }
                advance_item(ctx, item);
            }
            Err(message) => {
                op_arc.lock().expect("op lock").error_message = Some(message);
            }
        }
        return;
    }

    if matches!(kind, OpKind::Compress) {
        wait_while_paused(op_arc, ctx.resolver);
        if op_arc
            .lock()
            .expect("op lock")
            .cancel
            .load(Ordering::Relaxed)
        {
            return;
        }
        match compress_item_with_progress(&source, &destination_dir, item, ctx) {
            Ok(()) => {
                if op_arc
                    .lock()
                    .expect("op lock")
                    .cancel
                    .load(Ordering::Relaxed)
                {
                    return;
                }
                advance_item(ctx, item);
            }
            Err(message) => {
                op_arc.lock().expect("op lock").error_message = Some(message);
            }
        }
        return;
    }

    if matches!(kind, OpKind::Extract) {
        wait_while_paused(op_arc, ctx.resolver);
        if op_arc
            .lock()
            .expect("op lock")
            .cancel
            .load(Ordering::Relaxed)
        {
            return;
        }
        match extract_item_with_progress(&source, &destination_dir, item, ctx) {
            Ok(()) => {
                if op_arc
                    .lock()
                    .expect("op lock")
                    .cancel
                    .load(Ordering::Relaxed)
                {
                    return;
                }
                advance_item(ctx, item);
            }
            Err(message) => {
                op_arc.lock().expect("op lock").error_message = Some(message);
            }
        }
        return;
    }

    let mut target = destination_dir.join(&item.name);

    if source.is_dir() && is_nested_copy_target(&source, &target) {
        let action = match kind {
            OpKind::Copy => "copy",
            OpKind::Move => "move",
            // Unreachable: non-transfer kinds return earlier.
            OpKind::Delete => "delete",
            OpKind::Compress => "compress",
            OpKind::Extract => "extract",
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
                count_skipped_bytes(ctx, item.size_bytes);
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

    let add_discovered_totals = item.size_bytes == 0;
    let result = if matches!(kind, OpKind::Move) {
        move_path_with_total_discovery(&source, &target, add_discovered_totals, ctx)
    } else {
        copy_path_with_total_discovery(&source, &target, add_discovered_totals, ctx)
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

#[cfg(feature = "test-utils")]
pub fn process_item_for_tests(
    ctx: &WorkerCtx<'_>,
    item: &OpItem,
    conflict: Option<Arc<dyn Fn(ConflictInfo) + Send + Sync>>,
) {
    let conflict: Option<ConflictEmitter> = conflict;
    process_item(ctx, item, &conflict);
}

#[cfg(feature = "test-utils")]
pub fn archive_stem_for_item_for_tests(source: &Path) -> String {
    archive_stem_for_item(source)
}

#[cfg(feature = "test-utils")]
pub fn archive_root_name_for_tests(source: &Path) -> PathBuf {
    archive_root_name(source)
}

#[cfg(feature = "test-utils")]
pub fn begin_file_progress_for_tests(ctx: &WorkerCtx<'_>, source: &Path, total_bytes: u64) {
    begin_file_progress(ctx, source, total_bytes);
}

fn measure_delete_totals_up_front(
    items: &[OpItem],
    op_arc: &Arc<Mutex<OpState>>,
    progress: &Option<ProgressEmitter>,
) {
    let (kind, cancel) = {
        let op = op_arc.lock().expect("op lock");
        (op.kind, op.cancel.clone())
    };
    if !matches!(kind, OpKind::Delete) {
        return;
    }

    let mut discovered_bytes = 0_u64;
    for item in items.iter().filter(|item| item.size_bytes == 0) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        discovered_bytes = discovered_bytes.saturating_add(measure_delete_size(
            &PathBuf::from(&item.source_path),
            &cancel,
        ));
    }

    if discovered_bytes == 0 || cancel.load(Ordering::Relaxed) {
        return;
    }

    let mut op = op_arc.lock().expect("op lock");
    op.total_bytes = op.total_bytes.saturating_add(discovered_bytes);
    if let Some(emitter) = progress {
        emitter(op.progress());
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

fn advance_item(ctx: &WorkerCtx<'_>, _item: &OpItem) {
    let mut op = ctx.op_arc.lock().expect("op lock");
    op.completed_items += 1;
    op.current_file_name = None;
    op.current_file_copied = 0;
    op.current_file_total = 0;
    refresh_rate_locked_with_now(&mut op, ctx.start, ctx.rate_window, (ctx.instant_now)());

    if let Some(emitter) = ctx.progress {
        emitter(op.progress());
    }
}

fn count_skipped_bytes(ctx: &WorkerCtx<'_>, skipped_bytes: u64) {
    if skipped_bytes == 0 {
        return;
    }
    let mut op = ctx.op_arc.lock().expect("op lock");
    op.copied_bytes = op.copied_bytes.saturating_add(skipped_bytes);
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

fn add_discovered_total(ctx: &WorkerCtx<'_>, discovered_bytes: u64) {
    if discovered_bytes == 0 {
        return;
    }
    let mut op = ctx.op_arc.lock().expect("op lock");
    op.total_bytes = op.total_bytes.saturating_add(discovered_bytes);
    if let Some(emitter) = ctx.progress {
        emitter(op.progress());
    }
}

fn begin_file_progress(ctx: &WorkerCtx<'_>, source: &Path, total_bytes: u64) {
    let mut op = ctx.op_arc.lock().expect("op lock");
    op.current_file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .or_else(|| Some(source.to_string_lossy().into_owned()));
    op.current_file_copied = 0;
    op.current_file_total = total_bytes;
    if let Some(emitter) = ctx.progress {
        emitter(op.progress());
    }
}

fn finish_file_progress(ctx: &WorkerCtx<'_>) {
    let mut op = ctx.op_arc.lock().expect("op lock");
    op.copied_bytes = op.copied_bytes.saturating_add(op.current_file_copied);
    op.current_file_name = None;
    op.current_file_copied = 0;
    op.current_file_total = 0;
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
fn release_and_reschedule(id: &str, inner: &Arc<Mutex<Inner>>, runtime: &WorkerRuntime) {
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
        let runtime = runtime.clone();
        thread::spawn(move || {
            run_operation(next.clone(), &inner, &runtime);
            release_and_reschedule(&next, &inner, &runtime);
        });
    }

    // Auto-remove completed/cancelled operations after the retention window.
    schedule_auto_remove(id, inner, runtime.removed.clone(), runtime.retention);
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
            guard.signals.remove(&id);
            guard.order.retain(|entry| entry != &id);
            drop(guard);
            // Tell the UI to prune the card now that the backend forgot the op.
            if let Some(emitter) = &removed {
                emitter(id);
            }
        }
    });
}

fn compress_item_with_progress(
    source: &Path,
    archive_path: &Path,
    item: &OpItem,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    if !is_zip_archive_path(archive_path) {
        return Err("Archive output must end in .zip.".to_string());
    }
    if !source.exists() {
        return Err(format!(
            "Archive source does not exist: {}",
            source.display()
        ));
    }
    if archive_path.exists() {
        return Err(format!(
            "Archive already exists: {}",
            archive_path.display()
        ));
    }

    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let archive_file = fs::File::create_new(archive_path).map_err(|error| error.to_string())?;
    let mut writer = ZipWriter::new(archive_file);
    let add_discovered_totals = item.size_bytes == 0;
    let root_name = archive_root_name(source);
    append_archive_path(&mut writer, source, &root_name, add_discovered_totals, ctx)?;
    writer.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn extract_item_with_progress(
    source: &Path,
    destination_dir: &Path,
    item: &OpItem,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    if destination_dir.exists() {
        if !destination_dir.is_dir() {
            return Err(format!(
                "Extract destination is not a folder: {}",
                destination_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(destination_dir).map_err(|error| error.to_string())?;
    }
    if !is_zip_archive_path(source) {
        return Err("Only .zip archives can be extracted.".to_string());
    }

    let output_root =
        unique_archive_directory_path(destination_dir, &archive_stem_for_item(source));
    let discovered_total = zip_uncompressed_size(source)?;
    if item.size_bytes == 0 {
        add_discovered_total(ctx, discovered_total);
    }

    fs::create_dir_all(&output_root).map_err(|error| error.to_string())?;
    let file = fs::File::open(source).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let wrapper_root = detect_redundant_wrapper_root(&mut archive, &archive_stem_for_item(source))?;

    for index in 0..archive.len() {
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

        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let enclosed_name = entry
            .enclosed_name()
            .ok_or_else(|| "Archive contains an unsafe entry path.".to_string())?;
        let relative_name = strip_wrapper_root(&enclosed_name, wrapper_root.as_deref());
        let output_path = output_root.join(relative_name);

        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| error.to_string())?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let entry_size = entry.size();
        begin_file_progress(ctx, &output_path, entry_size);
        let mut output_file = fs::File::create(&output_path).map_err(|error| error.to_string())?;
        copy_reader_with_progress(&mut entry, &mut output_file, ctx)?;
        output_file.flush().map_err(|error| error.to_string())?;
        finish_file_progress(ctx);
    }

    Ok(())
}

fn append_archive_path(
    writer: &mut ZipWriter<fs::File>,
    source: &Path,
    archive_path: &Path,
    add_discovered_totals: bool,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
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

    if source.is_dir() {
        let mut directory_name = to_zip_path(archive_path)?;
        if !directory_name.ends_with('/') {
            directory_name.push('/');
        }
        writer
            .add_directory(directory_name, zip_dir_options())
            .map_err(|error| error.to_string())?;

        let mut children = fs::read_dir(source)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        children.sort_by_key(|entry| entry.file_name());

        for child in children {
            let child_source = child.path();
            let child_archive_path = archive_path.join(child.file_name());
            append_archive_path(
                writer,
                &child_source,
                &child_archive_path,
                add_discovered_totals,
                ctx,
            )?;
        }
        return Ok(());
    }

    let file_size = fs::metadata(source)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if add_discovered_totals {
        add_discovered_total(ctx, file_size);
    }
    begin_file_progress(ctx, source, file_size);
    writer
        .start_file(to_zip_path(archive_path)?, zip_file_options())
        .map_err(|error| error.to_string())?;
    let mut input_file = fs::File::open(source).map_err(|error| error.to_string())?;
    copy_reader_with_progress(&mut input_file, writer, ctx)?;
    finish_file_progress(ctx);
    Ok(())
}

fn copy_reader_with_progress<R, W>(
    reader: &mut R,
    writer: &mut W,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String>
where
    R: Read,
    W: Write,
{
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
    Ok(())
}

fn zip_uncompressed_size(path: &Path) -> Result<u64, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if !entry.is_dir() {
            total = total.saturating_add(entry.size());
        }
    }
    Ok(total)
}

fn detect_redundant_wrapper_root(
    archive: &mut ZipArchive<fs::File>,
    archive_stem: &str,
) -> Result<Option<String>, String> {
    let normalized_stem = archive_stem.trim();
    if normalized_stem.is_empty() {
        return Ok(None);
    }

    let mut candidate_root: Option<String> = None;
    let mut saw_nested_entry = false;

    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(enclosed_name) = entry.enclosed_name() else {
            return Err("Archive contains an unsafe entry path.".to_string());
        };

        let mut components = enclosed_name.components();
        let Some(first) = components.next() else {
            continue;
        };
        let Some(first_name) = first.as_os_str().to_str() else {
            return Ok(None);
        };
        if candidate_root
            .as_deref()
            .is_some_and(|current| current != first_name)
        {
            return Ok(None);
        }
        candidate_root.get_or_insert_with(|| first_name.to_string());
        if components.next().is_some() {
            saw_nested_entry = true;
        } else if !entry.is_dir() {
            return Ok(None);
        }
    }

    Ok(
        candidate_root
            .filter(|root| saw_nested_entry && root.eq_ignore_ascii_case(normalized_stem)),
    )
}

fn strip_wrapper_root<'a>(path: &'a Path, wrapper_root: Option<&str>) -> &'a Path {
    let Some(wrapper_root) = wrapper_root else {
        return path;
    };

    let mut components = path.components();
    let Some(first) = components.next() else {
        return path;
    };
    if !first
        .as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(wrapper_root)
    {
        return path;
    }

    components.as_path()
}

fn archive_stem_for_item(source: &Path) -> String {
    source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            source
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("Archive")
        .to_string()
}

fn archive_root_name(source: &Path) -> PathBuf {
    source
        .file_name()
        .map(PathBuf::from)
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("Archive"))
}

fn is_zip_archive_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
}

fn unique_archive_directory_path(destination_dir: &Path, stem: &str) -> PathBuf {
    let mut attempt = 0usize;
    loop {
        let file_name = if attempt == 0 {
            stem.to_string()
        } else {
            format!("{stem} ({attempt})")
        };
        let candidate = destination_dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

fn to_zip_path(path: &Path) -> Result<String, String> {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() {
        return Err("Archive entry path is empty.".to_string());
    }
    Ok(value)
}

fn zip_file_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

fn zip_dir_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o755)
}

// --- Filesystem helpers --------------------------------------------------------

/// Sum the byte size of a directory tree to drive byte-accurate progress.
/// Symlinks are not followed (consistent with the copy's link guard, and to stay
/// cycle-safe) and IO errors are skipped so measurement never aborts the op.
pub fn measure_tree_size(path: &Path, cancel: &Arc<AtomicBool>) -> u64 {
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

fn measure_delete_size(path: &Path, cancel: &Arc<AtomicBool>) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            measure_tree_size(path, cancel)
        }
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    }
}

pub fn copy_path(source: &Path, target: &Path, ctx: &WorkerCtx<'_>) -> Result<(), String> {
    copy_path_with_total_discovery(source, target, false, ctx)
}

fn copy_path_with_total_discovery(
    source: &Path,
    target: &Path,
    add_discovered_totals: bool,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    if source.is_dir() {
        let root = source.canonicalize().map_err(|error| error.to_string())?;
        let mut visited = HashSet::from([root.clone()]);
        copy_dir_recursive(
            source,
            target,
            &root,
            &mut visited,
            add_discovered_totals,
            ctx,
        )
    } else {
        copy_file_with_total_discovery(source, target, add_discovered_totals, ctx)
    }
}

pub fn copy_dir_recursive(
    source: &Path,
    target: &Path,
    root: &Path,
    visited: &mut HashSet<PathBuf>,
    add_discovered_totals: bool,
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
            copy_dir_recursive(
                &entry.path(),
                &child_target,
                root,
                visited,
                add_discovered_totals,
                ctx,
            )?;
            visited.remove(&canonical_child);
        } else {
            copy_file_with_total_discovery(
                &entry.path(),
                &child_target,
                add_discovered_totals,
                ctx,
            )?;
        }
    }
    Ok(())
}

/// Remove `source` (file or directory tree) reporting byte progress through the
/// shared worker context, honoring pause/cancel. When `add_discovered_totals`
/// is set (a directory whose size was not pre-measured), each removed file's
/// bytes are folded into the operation total as they are discovered.
pub fn delete_path_with_progress(
    source: &Path,
    add_discovered_totals: bool,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    let mut throttle = DeleteThrottle::new((ctx.instant_now)(), DELETE_EMIT_INTERVAL);
    delete_path_with_progress_throttled(source, add_discovered_totals, ctx, &mut throttle)
}

fn delete_path_with_progress_throttled(
    source: &Path,
    add_discovered_totals: bool,
    ctx: &WorkerCtx<'_>,
    throttle: &mut DeleteThrottle,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        delete_dir_recursive(source, add_discovered_totals, ctx, throttle)
    } else {
        delete_file_with_progress(source, add_discovered_totals, ctx, throttle)
    }
}

fn delete_dir_recursive(
    source: &Path,
    add_discovered_totals: bool,
    ctx: &WorkerCtx<'_>,
    throttle: &mut DeleteThrottle,
) -> Result<(), String> {
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
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() && !file_type.is_symlink() {
            delete_dir_recursive(&entry.path(), add_discovered_totals, ctx, throttle)?;
        } else {
            delete_file_with_progress(&entry.path(), add_discovered_totals, ctx, throttle)?;
        }
    }

    if ctx
        .op_arc
        .lock()
        .expect("op lock")
        .cancel
        .load(Ordering::Relaxed)
    {
        return Ok(());
    }

    fs::remove_dir(source).map_err(|error| error.to_string())
}

fn delete_file_with_progress(
    source: &Path,
    add_discovered_totals: bool,
    ctx: &WorkerCtx<'_>,
    throttle: &mut DeleteThrottle,
) -> Result<(), String> {
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

    let file_size = fs::symlink_metadata(source)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    fs::remove_file(source).map_err(|error| error.to_string())?;

    {
        let mut op = ctx.op_arc.lock().expect("op lock");
        if add_discovered_totals {
            op.total_bytes = op.total_bytes.saturating_add(file_size);
        }
        op.copied_bytes = op.copied_bytes.saturating_add(file_size);
        op.current_file_name = None;
        op.current_file_copied = 0;
        op.current_file_total = 0;
        refresh_rate_locked_with_now(&mut op, ctx.start, ctx.rate_window, (ctx.instant_now)());
    }
    maybe_emit_delete(ctx, throttle);
    Ok(())
}

fn maybe_emit_delete(ctx: &WorkerCtx<'_>, throttle: &mut DeleteThrottle) {
    let now = (ctx.instant_now)();
    if now.saturating_duration_since(throttle.last_emit) < throttle.interval {
        return;
    }

    throttle.last_emit = now;
    let op = ctx.op_arc.lock().expect("op lock");
    if let Some(emitter) = ctx.progress {
        emitter(op.progress());
    }
}

pub fn move_path(source: &Path, target: &Path, ctx: &WorkerCtx<'_>) -> Result<(), String> {
    move_path_with_total_discovery(source, target, false, ctx)
}

fn move_path_with_total_discovery(
    source: &Path,
    target: &Path,
    add_discovered_totals: bool,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-volume move: copy then delete the source.
            copy_path_with_total_discovery(source, target, add_discovered_totals, ctx)?;
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

pub fn copy_file_with_progress(
    source: &Path,
    target: &Path,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    copy_file_with_total_discovery(source, target, false, ctx)
}

fn copy_file_with_total_discovery(
    source: &Path,
    target: &Path,
    add_discovered_total_bytes: bool,
    ctx: &WorkerCtx<'_>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let file_size = fs::metadata(source)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if add_discovered_total_bytes {
        add_discovered_total(ctx, file_size);
    }
    begin_file_progress(ctx, source, file_size);

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
            finish_file_progress(ctx);
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

    writer.flush().map_err(|error| error.to_string())?;
    finish_file_progress(ctx);
    Ok(())
}

pub fn remove_target(target: &Path) -> Result<(), String> {
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(|error| error.to_string())
    } else {
        fs::remove_file(target).map_err(|error| error.to_string())
    }
}

pub fn remove_source(source: &Path) -> Result<(), String> {
    if source.is_dir() {
        fs::remove_dir_all(source).map_err(|error| error.to_string())
    } else {
        fs::remove_file(source).map_err(|error| error.to_string())
    }
}

pub fn unique_name(dir: &Path, name: &str) -> String {
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

pub fn split_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(index) if index > 0 => (name[..index].to_string(), name[index + 1..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}

pub fn parent_dir(path: &str) -> String {
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

pub fn fallback_root(path_text: &str) -> String {
    if let Some(index) = path_text.find(':') {
        return path_text[..=index].to_string();
    }
    let trimmed = path_text.trim_start_matches('/');
    match trimmed.split('/').next() {
        Some(first) if !first.is_empty() => format!("/{first}"),
        _ => "/".to_string(),
    }
}

pub fn is_nested_copy_target(source: &Path, target: &Path) -> bool {
    let source_components = normalized_components(source);
    let target_components = normalized_components(target);

    target_components.len() > source_components.len()
        && target_components.starts_with(&source_components)
}

pub fn normalized_components(path: &Path) -> Vec<String> {
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
