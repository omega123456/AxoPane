use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

use super::delete::remove_target;
use super::execution::{record_transfer_failure, ItemTransfer, TransferFailure, WorkerContext};
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

pub(super) struct ConflictCandidate<'candidate> {
    pub(super) item: &'candidate OpItem,
    pub(super) target: &'candidate Path,
}

pub(super) struct ParkedResolution {
    pub(super) resolution: ConflictResolution,
    pub(super) rename_to: Option<String>,
}

struct ConflictDecision {
    target: PathBuf,
    parked: ParkedResolution,
}

pub(super) fn resolve_transfer_target(
    context: &WorkerContext<'_>,
    conflict: &Option<ConflictEmitter>,
    transfer: &ItemTransfer,
) -> Option<TransferTarget> {
    let item = transfer.item;
    let target = transfer.destination_dir.join(&item.name);
    if !path_entry_exists(&target) {
        return Some(TransferTarget::separate(target));
    }
    let candidate = ConflictCandidate {
        item,
        target: &target,
    };
    let parked = resolve_or_park(context, candidate, conflict)?;
    apply_conflict_decision(context, transfer, ConflictDecision { target, parked })
}

fn apply_conflict_decision(
    context: &WorkerContext<'_>,
    transfer: &ItemTransfer,
    decision: ConflictDecision,
) -> Option<TransferTarget> {
    let item = transfer.item;
    let ConflictDecision { target, parked } = decision;
    match parked.resolution {
        ConflictResolution::Skip => {
            skip_conflicting_item(context, item);
            None
        }
        ConflictResolution::Replace => replace_target(context, transfer, target),
        ConflictResolution::Rename => {
            let new_name = parked
                .rename_to
                .unwrap_or_else(|| unique_name(&transfer.destination_dir, &item.name));
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

fn replace_target(
    context: &WorkerContext<'_>,
    transfer: &ItemTransfer,
    target: PathBuf,
) -> Option<TransferTarget> {
    let item = transfer.item;
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
            TransferFailure {
                kind: transfer.kind,
                source: &transfer.source,
                target: &target,
                error: &error,
            },
        );
        return None;
    }
    Some(TransferTarget::separate(target))
}

pub(super) fn resolve_or_park(
    context: &WorkerContext<'_>,
    candidate: ConflictCandidate<'_>,
    conflict: &Option<ConflictEmitter>,
) -> Option<ParkedResolution> {
    let ConflictCandidate { item, target } = candidate;
    let shared_operation = context.shared_operation;
    {
        let operation = shared_operation.lock_operation();
        if let Some(blanket) = operation.runtime.apply_to_all {
            return Some(ParkedResolution {
                resolution: blanket,
                rename_to: None,
            });
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
        operation.runtime.conflict = Some(info.clone());
        operation.runtime.conflict_resolution = None;
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
        if let Some(resolution) = guard.runtime.conflict_resolution.take() {
            let rename_to = guard.runtime.rename_to.take();
            return Some(ParkedResolution {
                resolution,
                rename_to,
            });
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
