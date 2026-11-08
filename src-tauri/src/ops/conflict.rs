use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

use super::delete::remove_target;
use super::execution::{record_transfer_failure, ItemTransfer, WorkerContext};
use super::paths::{is_real_directory, normalized_components, path_entry_exists, unique_name};
use super::progress::{advance_item, count_skipped_bytes, emit_operation_progress};
use super::{
    ConflictEmitter, ConflictInfo, ConflictResolution, LockOperation, OpItem, OpState, OpStatus,
};

pub(super) struct TransferTarget {
    pub(super) path: PathBuf,
    pub(super) merge_directories: bool,
}

impl TransferTarget {
    fn separate(path: PathBuf) -> Self {
        Self {
            path,
            merge_directories: false,
        }
    }
}

/// Picks where an item lands. `None` means the item needs no transfer: it was
/// skipped, failed while clearing the way, or was cancelled while parked.
pub(super) fn resolve_transfer_target(
    context: &WorkerContext<'_>,
    item: &OpItem,
    conflict: &Option<ConflictEmitter>,
    transfer: &ItemTransfer,
) -> Option<TransferTarget> {
    let target = transfer.destination_dir.join(&item.name);
    if !path_entry_exists(&target) {
        return Some(TransferTarget::separate(target));
    }
    let (resolution, rename_to) = resolve_or_park(context, item, &target, conflict)?;
    apply_conflict_decision(context, item, transfer, target, resolution, rename_to)
}

fn apply_conflict_decision(
    context: &WorkerContext<'_>,
    item: &OpItem,
    transfer: &ItemTransfer,
    target: PathBuf,
    resolution: ConflictResolution,
    rename_to: Option<String>,
) -> Option<TransferTarget> {
    match resolution {
        ConflictResolution::Skip => {
            skip_conflicting_item(context, item);
            None
        }
        ConflictResolution::Replace => replace_target(context, item, transfer, target),
        ConflictResolution::Rename => {
            let new_name =
                rename_to.unwrap_or_else(|| unique_name(&transfer.destination_dir, &item.name));
            Some(TransferTarget::separate(
                transfer.destination_dir.join(new_name),
            ))
        }
    }
}

fn skip_conflicting_item(context: &WorkerContext<'_>, item: &OpItem) {
    count_skipped_bytes(context, item.size_bytes);
    advance_item(context, item);
}

/// Replacing an item with itself would delete the source, so that case is a
/// skip. Folder onto folder merges (Explorer semantics); the copy/move walk
/// overwrites conflicting children and keeps the rest.
fn replace_target(
    context: &WorkerContext<'_>,
    item: &OpItem,
    transfer: &ItemTransfer,
    target: PathBuf,
) -> Option<TransferTarget> {
    let targets_source = normalized_components(&transfer.source) == normalized_components(&target);
    if targets_source {
        skip_conflicting_item(context, item);
        return None;
    }
    let merge_directories = transfer.source_is_real_directory && is_real_directory(&target);
    if merge_directories {
        return Some(TransferTarget {
            path: target,
            merge_directories,
        });
    }
    if let Err(error) = remove_target(&target) {
        record_transfer_failure(
            context.shared_operation,
            transfer.kind,
            &transfer.source,
            &target,
            &error,
        );
        return None;
    }
    Some(TransferTarget::separate(target))
}

/// Surface a conflict and block until the user resolves it (or it is cancelled).
pub(super) fn resolve_or_park(
    context: &WorkerContext<'_>,
    item: &OpItem,
    target: &Path,
    conflict: &Option<ConflictEmitter>,
) -> Option<(ConflictResolution, Option<String>)> {
    let shared_operation = context.shared_operation;
    {
        let operation = shared_operation.lock_operation();
        if let Some(blanket) = operation.apply_to_all {
            return Some((blanket, None));
        }
    }

    let info = ConflictInfo {
        operation_id: shared_operation.lock_operation().id.clone(),
        source_path: item.source_path.clone(),
        destination_path: target.to_string_lossy().into_owned(),
        name: item.name.clone(),
    };

    {
        let mut operation = shared_operation.lock_operation();
        operation.status = OpStatus::Conflict;
        operation.conflict = Some(info.clone());
        operation.conflict_resolution = None;
        emit_operation_progress(context.progress, &operation);
    }
    if let Some(emitter) = conflict {
        emitter(info);
    }

    let mut guard = shared_operation.lock_operation();
    loop {
        if guard.cancel.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(resolution) = guard.conflict_resolution.take() {
            let rename = guard.rename_to.take();
            return Some((resolution, rename));
        }
        guard = context.resolver.wait(guard).expect("conflict wait");
    }
}

pub(super) fn wait_while_paused(shared_operation: &Arc<Mutex<OpState>>, resolver: &Arc<Condvar>) {
    let mut guard = shared_operation.lock_operation();
    while guard.is_held_by_pause() {
        guard = resolver.wait(guard).expect("pause wait");
    }
}
