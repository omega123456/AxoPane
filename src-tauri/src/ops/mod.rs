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

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub mod archive;
mod conflict;
mod copy;
mod delete;
mod execution;
pub mod manifest;
mod measure;
mod paths;
mod progress;
mod scheduler;
mod service;
pub mod status;
pub mod transfer;
pub mod worker;

#[cfg(feature = "test-utils")]
pub use copy::adapter_selection_calls_for_tests;
pub use copy::{copy_dir_recursive, copy_file_with_progress, copy_path, move_path, TreeCopy};
pub use delete::{delete_path_with_progress, remove_source, remove_target};
pub use execution::WorkerContext;
#[cfg(feature = "test-utils")]
pub use execution::{
    archive_root_name_for_tests, archive_stem_for_item_for_tests, begin_file_progress_for_tests,
    process_item_for_tests,
};
pub use measure::measure_tree_size;
pub use paths::{
    fallback_root, is_nested_copy_target, normalized_components, parent_dir, split_name,
    unique_name, volume_root_for,
};
pub use progress::TransferThrottle;
pub use service::{requested_pop, OpsService};
use status::RunOutcome;

/// How long a completed or cancelled operation lingers in the queue before being auto-removed.
pub const DEFAULT_COMPLETED_RETENTION: Duration = Duration::from_secs(4);
/// How often active operations recompute their short-window instantaneous rate.
pub const DEFAULT_RATE_WINDOW: Duration = Duration::from_millis(250);
/// Minimum spacing between progress emissions while deleting individual files,
/// or while copying/moving the files nested inside a directory tree.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(90);
const COPY_BUFFER_BYTES: usize = 1024 * 1024;
const PERCENT_SCALE: f64 = 100.0;

/// Top-level item names carried on every `OpProgress`. Enough for the card's
/// expandable selection list, small enough that a 500-item selection clones a
/// bounded handful of short strings per tick rather than the whole selection.
const ITEM_NAME_PREVIEW_LIMIT: usize = 24;

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

impl OpKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Move => "move",
            Self::Delete => "delete",
            Self::Compress => "compress",
            Self::Extract => "extract",
        }
    }

    pub fn is_transfer(self) -> bool {
        matches!(self, Self::Copy | Self::Move)
    }
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

struct ConflictDecision {
    resolution: ConflictResolution,
    apply_to_all: bool,
    rename_to: Option<String>,
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

trait LockOperation {
    fn lock_operation(&self) -> MutexGuard<'_, OpState>;
}

impl LockOperation for Mutex<OpState> {
    fn lock_operation(&self) -> MutexGuard<'_, OpState> {
        self.lock().expect("op lock")
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
    /// One-shot request to abandon the current top-level item while paused.
    pub skip: Arc<AtomicBool>,
    /// Pending conflict, and the channel the worker waits on for resolution.
    pub conflict: Option<ConflictInfo>,
    pub conflict_resolution: Option<ConflictResolution>,
    pub apply_to_all: Option<ConflictResolution>,
    pub rename_to: Option<String>,
}

impl OpState {
    fn progress_percent(&self, visible_copied_bytes: u64) -> f64 {
        if self.total_bytes != 0 {
            return (visible_copied_bytes as f64 / self.total_bytes as f64) * PERCENT_SCALE;
        }
        if matches!(self.status, OpStatus::Completed) {
            return PERCENT_SCALE;
        }
        0.0
    }

    pub fn progress(&self) -> OpProgress {
        let visible_copied_bytes = self.visible_copied_bytes();
        let progress_percent = self.progress_percent(visible_copied_bytes);

        OpProgress {
            operation_id: self.id.clone(),
            kind: self.kind,
            status: self.status,
            source_dir: self
                .items
                .first()
                .map(|item| parent_dir(&item.source_path))
                .unwrap_or_default(),
            // Bounded preview: the collapsed card shows the first two names
            // plus a "+K more" toggle, and expanding it lists what arrived
            // here. `total_items` (verified == `items.len()` at all times)
            // still carries the true count, so a selection larger than this
            // cap reports the remainder rather than pretending the list is
            // complete. Cloning every top-level name on every tick would be
            // pure waste on large selections.
            item_names: self
                .items
                .iter()
                .take(ITEM_NAME_PREVIEW_LIMIT)
                .map(|item| item.name.clone())
                .collect(),
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
        self.status.is_terminal()
    }

    fn request_cancel(&mut self) {
        self.skip.store(false, Ordering::Relaxed);
        self.cancel.store(true, Ordering::Relaxed);
        self.pause.store(false, Ordering::Relaxed);
    }

    fn run_outcome(&self) -> RunOutcome {
        if self.cancel.load(Ordering::Relaxed) {
            return RunOutcome::Cancelled;
        }
        if self.error_message.is_some() {
            return RunOutcome::Failed;
        }
        RunOutcome::Completed
    }

    fn clear_current_file(&mut self) {
        self.current_file_name = None;
        self.current_file_copied = 0;
        self.current_file_total = 0;
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
        self.clear_current_file();
        self.error_message = None;
        self.completed_at = None;
        self.skip.store(false, Ordering::Relaxed);
        self.conflict = None;
        self.conflict_resolution = None;
        self.apply_to_all = None;
        self.rename_to = None;
    }
}
