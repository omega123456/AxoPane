use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::execution::WorkerContext;
use super::measure::measure_path_size;
use super::{
    LockOperation, OpItem, OpKind, OpState, ProgressEmitter, ETA_STABILIZATION_SAMPLES,
    PROGRESS_EMIT_INTERVAL,
};

/// Coalesces the flood of per-chunk / per-member progress emissions produced
/// while transferring bytes. Every transfer-shaped path carries one of these:
/// the recursive directory walk (`copy_dir_recursive`) shares a single
/// throttle across the whole tree; the single top-level file transfer (the
/// non-directory branch of `copy_path_with_total_discovery`) gets its own
/// throttle, since a large single file floods exactly the same way a
/// directory walk does; and the compress/extract member loops
/// (`compress_item_with_progress`, `extract_item_with_progress`) share one
/// throttle across every member of the operation so a many-small-member
/// archive does not re-flood by minting a fresh throttle per member.
pub struct TransferThrottle {
    last_emit: Instant,
    interval: Duration,
}

impl TransferThrottle {
    pub(super) fn new(last_emit: Instant, interval: Duration) -> Self {
        Self {
            last_emit,
            interval,
        }
    }

    pub(super) fn starting_now(context: &WorkerContext<'_>) -> Self {
        Self::new((context.instant_now)(), PROGRESS_EMIT_INTERVAL)
    }

    fn claim_emission(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.last_emit) < self.interval {
            return false;
        }
        self.last_emit = now;
        true
    }
}

pub(super) fn emit_operation_progress(progress: &Option<ProgressEmitter>, operation: &OpState) {
    if let Some(emitter) = progress {
        emitter(operation.progress());
    }
}

/// Emit `operation`'s progress now, or — when a [`TransferThrottle`] is supplied —
/// only once `interval` has elapsed since the last emission for this walk.
fn maybe_emit_transfer(
    context: &WorkerContext<'_>,
    operation: &OpState,
    throttle: Option<&mut TransferThrottle>,
) {
    let emission_due =
        throttle.is_none_or(|throttle| throttle.claim_emission((context.instant_now)()));
    if emission_due {
        emit_operation_progress(context.progress, operation);
    }
}

/// Pre-measure the real byte total for items whose size arrived as 0 (folders,
/// whose size the frontend cannot cheaply know up front) before any work starts.
/// Without this, the total grows piecemeal as the walk discovers each nested
/// file, so `copied / total` can race ahead of 100% and get clamped there long
/// before the job is actually done — this keeps the denominator accurate from
/// the first progress emission for all transfer-shaped kinds.
pub(super) fn measure_totals_up_front(
    items: &[OpItem],
    shared_operation: &Arc<Mutex<OpState>>,
    progress: &Option<ProgressEmitter>,
) {
    let (kind, cancel) = {
        let operation = shared_operation.lock_operation();
        (operation.kind, operation.cancel.clone())
    };
    if !matches!(kind, OpKind::Delete | OpKind::Copy) {
        return;
    }

    // Copy preserves symlinks as links, so their own on-disk size belongs in
    // the total instead of the size of whatever they happen to point at.
    // Delete also removes the link itself, so neither operation follows them.
    let follow_symlinks = false;
    let mut discovered_bytes = 0_u64;
    for item in items.iter().filter(|item| item.size_bytes == 0) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        discovered_bytes = discovered_bytes.saturating_add(measure_path_size(
            &PathBuf::from(&item.source_path),
            &cancel,
            follow_symlinks,
        ));
    }

    if discovered_bytes == 0 || cancel.load(Ordering::Relaxed) {
        return;
    }

    let mut operation = shared_operation.lock_operation();
    operation.total_bytes = operation.total_bytes.saturating_add(discovered_bytes);
    emit_operation_progress(progress, &operation);
}

pub(super) fn advance_item(context: &WorkerContext<'_>, _item: &OpItem) {
    let mut operation = context.shared_operation.lock_operation();
    operation.completed_items += 1;
    operation.clear_current_file();
    refresh_rate_locked_with_now(
        &mut operation,
        context.start,
        context.rate_window,
        (context.instant_now)(),
    );

    emit_operation_progress(context.progress, &operation);
}

pub(super) fn count_skipped_bytes(context: &WorkerContext<'_>, skipped_bytes: u64) {
    if skipped_bytes == 0 {
        return;
    }
    let mut operation = context.shared_operation.lock_operation();
    operation.copied_bytes = operation.copied_bytes.saturating_add(skipped_bytes);
    emit_operation_progress(context.progress, &operation);
}

pub(super) fn report_chunk_progress(
    context: &WorkerContext<'_>,
    copied_delta: u64,
    throttle: Option<&mut TransferThrottle>,
) {
    let mut operation = context.shared_operation.lock_operation();
    operation.current_file_copied = operation.current_file_copied.saturating_add(copied_delta);
    refresh_rate_locked_with_now(
        &mut operation,
        context.start,
        context.rate_window,
        (context.instant_now)(),
    );
    maybe_emit_transfer(context, &operation, throttle);
}

pub(super) fn add_discovered_total(
    context: &WorkerContext<'_>,
    discovered_bytes: u64,
    throttle: Option<&mut TransferThrottle>,
) {
    if discovered_bytes == 0 {
        return;
    }
    let mut operation = context.shared_operation.lock_operation();
    operation.total_bytes = operation.total_bytes.saturating_add(discovered_bytes);
    maybe_emit_transfer(context, &operation, throttle);
}

pub(super) fn begin_file_progress(
    context: &WorkerContext<'_>,
    source: &Path,
    total_bytes: u64,
    throttle: Option<&mut TransferThrottle>,
) {
    let mut operation = context.shared_operation.lock_operation();
    operation.current_file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .or_else(|| Some(source.to_string_lossy().into_owned()));
    operation.current_file_copied = 0;
    operation.current_file_total = total_bytes;
    maybe_emit_transfer(context, &operation, throttle);
}

pub(super) fn finish_file_progress(
    context: &WorkerContext<'_>,
    throttle: Option<&mut TransferThrottle>,
) {
    let mut operation = context.shared_operation.lock_operation();
    operation.copied_bytes = operation
        .copied_bytes
        .saturating_add(operation.current_file_copied);
    operation.clear_current_file();
    refresh_rate_locked_with_now(
        &mut operation,
        context.start,
        context.rate_window,
        (context.instant_now)(),
    );
    maybe_emit_transfer(context, &operation, throttle);
}

pub(super) fn refresh_rate_locked_with_now(
    operation: &mut OpState,
    start: Instant,
    rate_window: Duration,
    now: Instant,
) {
    operation.sample_count += 1;
    let visible_copied = operation.visible_copied_bytes();
    let last_sample_at = operation.rate_sample_at.unwrap_or(start);
    let last_sample_bytes = operation.rate_sample_bytes;
    let should_recompute = rate_window.is_zero()
        || operation.bytes_per_second == 0
        || now.duration_since(last_sample_at) >= rate_window;

    if should_recompute {
        let sample_elapsed = now.duration_since(last_sample_at).as_secs_f64().max(0.001);
        let sample_bytes = visible_copied.saturating_sub(last_sample_bytes);
        operation.bytes_per_second = (sample_bytes as f64 / sample_elapsed) as u64;
        operation.rate_sample_at = Some(now);
        operation.rate_sample_bytes = visible_copied;
    }

    let stable_elapsed = start.elapsed().as_secs_f64().max(0.001);
    let stable_rate = (visible_copied as f64 / stable_elapsed) as u64;
    if operation.sample_count >= ETA_STABILIZATION_SAMPLES && stable_rate > 0 {
        let remaining = operation.total_bytes.saturating_sub(visible_copied);
        operation.eta_seconds = Some(remaining / stable_rate.max(1));
    }
}
pub(super) fn maybe_emit_delete(context: &WorkerContext<'_>, throttle: &mut TransferThrottle) {
    if !throttle.claim_emission((context.instant_now)()) {
        return;
    }
    let operation = context.shared_operation.lock_operation();
    emit_operation_progress(context.progress, &operation);
}
