use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[cfg(feature = "test-utils")]
use super::archive::{archive_root_name, archive_stem_for_item};
#[cfg(feature = "test-utils")]
use super::progress::begin_file_progress;
#[cfg(feature = "test-utils")]
use super::progress::FileProgressStart;
#[cfg(feature = "test-utils")]
use super::ConflictInfo;

use super::archive::{compress_item_with_progress, extract_item_with_progress};
use super::conflict::{resolve_transfer_target, wait_while_paused, TransferTarget};
use super::copy::{
    copy_path_with_total_discovery, move_merge_dir, move_path_with_total_discovery, TotalDiscovery,
};
use super::delete::delete_path_with_progress;
use super::paths::{is_nested_copy_target, is_real_directory};
use super::progress::{
    advance_item, emit_operation_progress, measure_totals_up_front, refresh_rate_locked_with_now,
    RateClock,
};
use super::service::Inner;
use super::status::RunOutcome;
use super::worker::TransferPaths;
use super::{
    ConflictEmitter, InstantNow, LockOperation, OpItem, OpKind, OpState, OpStatus, ProgressEmitter,
    RateSample, RemovedEmitter,
};

#[derive(Clone)]
pub(super) struct WorkerRuntime {
    pub(super) progress: Option<ProgressEmitter>,
    pub(super) conflict: Option<ConflictEmitter>,
    pub(super) removed: Option<RemovedEmitter>,
    pub(super) retention: Duration,
    pub(super) rate_window: Duration,
    pub(super) instant_now: InstantNow,
    pub(super) coordinator: Arc<crate::resource_coordinator::ResourceCoordinator>,
}

pub struct WorkerContext<'a> {
    pub shared_operation: &'a Arc<Mutex<OpState>>,
    pub resolver: &'a Arc<Condvar>,
    pub progress: &'a Option<ProgressEmitter>,
    pub start: Instant,
    pub rate_window: Duration,
    pub instant_now: &'a InstantNow,
}

impl WorkerContext<'_> {
    pub(super) fn rate_clock(&self) -> RateClock {
        RateClock {
            start: self.start,
            rate_window: self.rate_window,
            now: (self.instant_now)(),
        }
    }
}

#[derive(PartialEq, Eq)]
enum ItemFlow {
    Continue,
    Stop,
}

pub(super) struct ItemTransfer<'item> {
    pub(super) kind: OpKind,
    pub(super) item: &'item OpItem,
    pub(super) source: PathBuf,
    pub(super) destination_dir: PathBuf,
    pub(super) source_is_real_directory: bool,
}

pub(super) fn run_operation(id: String, inner: &Arc<Mutex<Inner>>, runtime: &WorkerRuntime) {
    let Some(OperationHandles {
        shared_operation,
        signal,
    }) = operation_handles(&id, inner)
    else {
        return;
    };
    let instant_now = runtime.instant_now.clone();
    mark_operation_active(&shared_operation, instant_now(), &runtime.progress);

    let items = shared_operation.lock_operation().items.clone();
    measure_totals_up_front(&items, &shared_operation, &runtime.progress);

    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &signal,
        progress: &runtime.progress,
        start: instant_now(),
        rate_window: runtime.rate_window,
        instant_now: &instant_now,
    };
    for item in &items {
        let flow = run_item(&context, item, &runtime.conflict);
        if flow == ItemFlow::Stop {
            break;
        }
    }

    finish_operation(&shared_operation, &runtime.progress);
}

struct OperationHandles {
    shared_operation: Arc<Mutex<OpState>>,
    signal: Arc<Condvar>,
}

fn operation_handles(id: &str, inner: &Arc<Mutex<Inner>>) -> Option<OperationHandles> {
    let guard = inner.lock().expect("ops lock");
    let shared_operation = guard.ops.get(id).cloned()?;
    let signal = guard.signals.get(id).cloned()?;
    Some(OperationHandles {
        shared_operation,
        signal,
    })
}

fn mark_operation_active(
    shared_operation: &Arc<Mutex<OpState>>,
    now: Instant,
    progress: &Option<ProgressEmitter>,
) {
    let mut operation = shared_operation.lock_operation();
    operation.status = OpStatus::Active;
    operation.runtime.rate_sample = Some(RateSample { at: now, bytes: 0 });
    emit_operation_progress(progress, &operation);
}

fn run_item(
    context: &WorkerContext<'_>,
    item: &OpItem,
    conflict: &Option<ConflictEmitter>,
) -> ItemFlow {
    let copied_before = context
        .shared_operation
        .lock_operation()
        .runtime
        .copied_bytes;
    if finish_skipped_item(context, item, copied_before) {
        return ItemFlow::Continue;
    }
    if is_operation_stopped(context) {
        return ItemFlow::Stop;
    }

    process_item(context, item, conflict);

    if finish_skipped_item(context, item, copied_before) {
        return ItemFlow::Continue;
    }
    if is_operation_stopped(context) {
        return ItemFlow::Stop;
    }
    ItemFlow::Continue
}

fn is_operation_stopped(context: &WorkerContext<'_>) -> bool {
    let operation = context.shared_operation.lock_operation();
    operation.cancel.load(Ordering::Relaxed) || operation.runtime.error_message.is_some()
}

fn finish_operation(shared_operation: &Arc<Mutex<OpState>>, progress: &Option<ProgressEmitter>) {
    let mut operation = shared_operation.lock_operation();
    let outcome = operation.run_outcome();
    operation.status = OpStatus::finish(outcome);
    if outcome == RunOutcome::Completed {
        operation.runtime.copied_bytes = operation.total_bytes;
    }
    operation.runtime.completed_at = Some(Instant::now());
    operation.clear_current_file();
    emit_operation_progress(progress, &operation);
}

fn finish_skipped_item(context: &WorkerContext<'_>, item: &OpItem, copied_before: u64) -> bool {
    let mut operation = context.shared_operation.lock_operation();
    if !operation.take_skip_request() {
        return false;
    }

    operation.cancel.store(false, Ordering::Relaxed);
    operation.pause.store(true, Ordering::Relaxed);
    operation.status = OpStatus::Paused;
    operation.runtime.completed_items += 1;
    if item.size_bytes > 0 {
        operation.runtime.copied_bytes = operation
            .runtime
            .copied_bytes
            .max(copied_before.saturating_add(item.size_bytes));
    }
    operation.clear_current_file();
    refresh_rate_locked_with_now(&mut operation, context.rate_clock());
    emit_operation_progress(context.progress, &operation);
    true
}

pub(super) fn process_item(
    context: &WorkerContext<'_>,
    item: &OpItem,
    conflict: &Option<ConflictEmitter>,
) {
    let (kind, destination_dir) = {
        let operation = context.shared_operation.lock_operation();
        (operation.kind, operation.destination_dir.clone())
    };
    let source = PathBuf::from(&item.source_path);

    if let Some(work) = queue_native_item_work(kind) {
        let paths = NativeItemPaths {
            source: &source,
            target: &destination_dir,
        };
        run_queue_native_item(context, NativeItemJob { item, paths, work });
        return;
    }

    let transfer = ItemTransfer {
        kind,
        item,
        source_is_real_directory: is_real_directory(&source),
        source,
        destination_dir,
    };
    let requested_target = transfer.destination_dir.join(&item.name);
    let copies_into_itself = transfer.source_is_real_directory
        && is_nested_copy_target(&transfer.source, &requested_target);
    if copies_into_itself {
        record_nested_target_failure(context.shared_operation, &transfer);
        return;
    }

    let Some(target) = resolve_transfer_target(context, conflict, &transfer) else {
        return;
    };

    wait_while_paused(context.shared_operation, context.resolver);
    if is_operation_cancelled(context) {
        return;
    }

    match run_transfer(&transfer, &target, context) {
        Ok(()) => advance_unless_cancelled(context, item),
        Err(message) => record_transfer_failure(
            context.shared_operation,
            TransferFailure {
                kind,
                source: &transfer.source,
                target: &target.path,
                error: &message,
            },
        ),
    }
}

fn record_nested_target_failure(shared_operation: &Arc<Mutex<OpState>>, transfer: &ItemTransfer) {
    let action = transfer.kind.label();
    let mut operation = shared_operation.lock_operation();
    operation.runtime.error_message = Some(format!(
        "Cannot {action} \"{}\" into one of its own descendants.",
        transfer.source.display()
    ));
}

fn run_transfer(
    transfer: &ItemTransfer,
    target: &TransferTarget,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    let transfer_paths = TransferPaths {
        source: &transfer.source,
        target: &target.path,
    };
    if transfer.kind != OpKind::Move {
        return copy_path_with_total_discovery(
            transfer_paths,
            TotalDiscovery::known_totals(),
            context,
        );
    }
    if target.merge_directories {
        return move_merge_dir(&transfer.source, &target.path, context);
    }
    move_path_with_total_discovery(transfer_paths, false, context)
}

#[derive(Clone, Copy)]
pub(super) struct NativeItemPaths<'path> {
    pub(super) source: &'path Path,
    pub(super) target: &'path Path,
}

type QueueNativeItemWork =
    fn(NativeItemPaths<'_>, &OpItem, &WorkerContext<'_>) -> Result<(), String>;

struct NativeItemJob<'job> {
    item: &'job OpItem,
    paths: NativeItemPaths<'job>,
    work: QueueNativeItemWork,
}

fn queue_native_item_work(kind: OpKind) -> Option<QueueNativeItemWork> {
    match kind {
        OpKind::Delete => Some(delete_item),
        OpKind::Compress => Some(compress_item_with_progress),
        OpKind::Extract => Some(extract_item_with_progress),
        OpKind::Copy | OpKind::Move => None,
    }
}

fn delete_item(
    paths: NativeItemPaths<'_>,
    _item: &OpItem,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    let source = paths.source;
    delete_path_with_progress(source, false, context).inspect_err(|message| {
        log::error!(
            "delete operation failed: source=\"{}\", error={message}",
            source.display()
        );
    })
}

fn run_queue_native_item(context: &WorkerContext<'_>, job: NativeItemJob<'_>) {
    let NativeItemJob { item, paths, work } = job;
    wait_while_paused(context.shared_operation, context.resolver);
    if is_operation_cancelled(context) {
        return;
    }
    match work(paths, item, context) {
        Ok(()) => advance_unless_cancelled(context, item),
        Err(message) => {
            context
                .shared_operation
                .lock_operation()
                .runtime
                .error_message = Some(message)
        }
    }
}

fn advance_unless_cancelled(context: &WorkerContext<'_>, item: &OpItem) {
    if !is_operation_cancelled(context) {
        advance_item(context, item);
    }
}

pub(super) struct TransferFailure<'failure> {
    pub(super) kind: OpKind,
    pub(super) source: &'failure Path,
    pub(super) target: &'failure Path,
    pub(super) error: &'failure str,
}

pub(super) fn record_transfer_failure(
    shared_operation: &Arc<Mutex<OpState>>,
    failure: TransferFailure<'_>,
) {
    let TransferFailure {
        kind,
        source,
        target,
        error,
    } = failure;
    if !kind.is_transfer() {
        return;
    }
    let action = kind.label();
    let mut operation = shared_operation.lock_operation();
    let message = format!(
        "{action} operation {} failed: source=\"{}\", destination=\"{}\", error={error}",
        operation.id,
        source.display(),
        target.display(),
    );
    log::error!("{message}");
    operation.runtime.error_message = Some(format!(
        "Failed to {action} \"{}\" to \"{}\": {error}",
        source.display(),
        target.display(),
    ));
}

pub(super) fn is_operation_cancelled(context: &WorkerContext<'_>) -> bool {
    context
        .shared_operation
        .lock_operation()
        .cancel
        .load(Ordering::Relaxed)
}

#[cfg(feature = "test-utils")]
pub fn process_item_for_tests(
    context: &WorkerContext<'_>,
    item: &OpItem,
    conflict: Option<Arc<dyn Fn(ConflictInfo) + Send + Sync>>,
) {
    let conflict: Option<ConflictEmitter> = conflict;
    process_item(context, item, &conflict);
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
pub fn begin_file_progress_for_tests(context: &WorkerContext<'_>, source: &Path, total_bytes: u64) {
    begin_file_progress(
        context,
        FileProgressStart {
            source,
            total_bytes,
        },
        None,
    );
}
