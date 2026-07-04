// Pure, IO-free batching primitive shared by both `request_icons` code paths
// (the Windows rayon-pool path and the sequential fallback used on macOS, and
// the `test-utils` synchronous variant). It has no `AppHandle`/thread
// dependency, so it can be constructed and driven directly in tests without
// any Tauri runtime.
//
// Icons are flushed once at least `MAX_BATCH` have accumulated, or once
// `FLUSH_INTERVAL` has elapsed since the last flush — whichever comes first —
// so a large folder still streams its first batch quickly instead of waiting
// for every icon to resolve. Callers must also call
// `IconBatcher::drain_remainder` once after the last icon is pushed to flush
// any icons left in the buffer.
//
// NOTE: plain (non-doc) comments are used intentionally so this file can be
// `include!`d verbatim into `ipc_icon_batch_integration.rs`'s whitebox test
// module for direct coverage of the time-based flush branch; a leading `//!`
// module doc comment is only valid as the first item in a file/module, which
// an `include!` site partway through a module is not.

use std::time::{Duration, Instant};

use crate::ipc::types::IconStateEvent;

/// Flush once at least this many icons have accumulated in the buffer.
pub(crate) const MAX_BATCH: usize = 64;

/// ...or once this much time has elapsed since the last flush, whichever
/// comes first.
pub(crate) const FLUSH_INTERVAL: Duration = Duration::from_millis(50);

/// Accumulates resolved [`IconStateEvent`]s and hands back a drained batch
/// once the count or time threshold is reached.
pub(crate) struct IconBatcher {
    buffer: Vec<IconStateEvent>,
    last_flush: Instant,
}

impl IconBatcher {
    /// Create a new, empty batcher whose flush clock starts at `now`.
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            buffer: Vec::new(),
            last_flush: now,
        }
    }

    /// Push a resolved icon event. Returns `Some(batch)` — draining the
    /// buffer — when the batch has reached [`MAX_BATCH`] items or
    /// [`FLUSH_INTERVAL`] has elapsed since the last flush; otherwise
    /// returns `None` and the event stays buffered.
    pub(crate) fn push(&mut self, event: IconStateEvent, now: Instant) -> Option<Vec<IconStateEvent>> {
        self.buffer.push(event);
        if self.buffer.len() >= MAX_BATCH || now.duration_since(self.last_flush) >= FLUSH_INTERVAL {
            self.last_flush = now;
            Some(std::mem::take(&mut self.buffer))
        } else {
            None
        }
    }

    /// Flush any icons left in the buffer after the last `push`. Returns
    /// `None` when the buffer is already empty (nothing to flush).
    pub(crate) fn drain_remainder(&mut self) -> Option<Vec<IconStateEvent>> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buffer))
        }
    }
}
