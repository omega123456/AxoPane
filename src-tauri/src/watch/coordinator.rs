//! Bounded watch traffic coordinator (Phase 5 / Functional Requirement 2).
//!
//! Platform `notify` callbacks push raw events into a fixed-capacity lane
//! ([`RAW_LANE_CAPACITY`]) using non-blocking [`crossbeam_channel::Sender::try_send`]
//! semantics — the OS callback thread must never block waiting on an
//! application consumer. Control messages (unwatch/replace/shutdown/explicit
//! refresh) travel on a separate, smaller bounded lane
//! ([`CONTROL_LANE_CAPACITY`]) so they can never queue behind a burst of
//! filesystem mutations.
//!
//! A background compactor thread drains both lanes, coalescing raw events by
//! `(watch_id, child_path)` identity, and flushes a compacted batch for a
//! given watch once either:
//! - [`QUIET_WINDOW`] has elapsed since the last event for that watch, or
//! - [`MAX_BATCH_AGE`] has elapsed since the first pending event for that
//!   watch (whichever comes first) — so continuous traffic still flushes
//!   periodically instead of waiting indefinitely for silence.
//!
//! Overflow protection: if the raw lane is full (a `try_send` failure) or a
//! single watch accumulates more than [`MAX_PENDING_MUTATIONS_PER_WATCH`]
//! distinct pending child paths, that watch's pending batch is discarded and
//! replaced with a single revisioned "dirty" marker instructing the caller to
//! perform one authoritative resnapshot — never growing the pending set
//! without bound and never blocking.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};

/// Fixed capacity of the raw notification lane. A `try_send` that finds this
/// lane full marks the affected watch dirty rather than blocking or growing
/// without bound (Functional Requirement 2).
pub const RAW_LANE_CAPACITY: usize = 4096;

/// Fixed capacity of the priority control lane (unwatch/replace/shutdown/
/// explicit refresh). Kept small and separate from the raw lane so control
/// work is never starved behind bulk mutation traffic.
pub const CONTROL_LANE_CAPACITY: usize = 64;

/// More than this many distinct pending child-mutation paths for one watch
/// converts that watch's pending batch into a single dirty/resnapshot
/// instruction, discarding the detailed per-path history for that
/// generation.
pub const MAX_PENDING_MUTATIONS_PER_WATCH: usize = 2048;

/// Compaction quiet window: a watch's pending batch flushes this long after
/// its most recent event, provided no newer event has arrived since.
pub const QUIET_WINDOW: Duration = Duration::from_millis(150);

/// Compaction maximum age: a watch's pending batch flushes this long after
/// its oldest still-pending event, even under continuous traffic that never
/// goes quiet.
pub const MAX_BATCH_AGE: Duration = Duration::from_millis(500);

/// Opaque identity for one watch registration. The coordinator does not
/// interpret this beyond using it as a compaction/grouping key; callers
/// (typically [`crate::directory_session`] or the legacy [`crate::watch`]
/// runtime) mint their own monotonic ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WatchId(pub u64);

/// One raw mutation observed for a watched child path. `kind` is a coarse
/// classification sufficient for compaction; the authoritative content is
/// always re-read from disk when a batch is flushed (or a resnapshot is
/// triggered), so this never needs to carry a full [`crate::fs::DirectoryEntry`].
#[derive(Debug, Clone)]
pub struct RawMutation {
    pub watch_id: WatchId,
    pub child_path: PathBuf,
    pub kind: MutationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    Changed,
    Removed,
    /// The event could not be resolved into a definite child identity
    /// (need_rescan, `Any`/`Other`, or an unresolvable rename) — this alone
    /// is enough to force the affected watch dirty regardless of the
    /// pending-mutation count.
    Unresolved,
}

/// A control-lane instruction. Always processed ahead of any pending raw
/// mutation batch for the same watch.
#[derive(Debug, Clone)]
pub enum ControlMessage {
    /// Stop tracking `watch_id` entirely; drops any pending compaction state.
    Unwatch(WatchId),
    /// Replace `watch_id`'s pending state with a clean slate (e.g. the
    /// watched path changed under the same logical watch).
    Replace(WatchId),
    /// Force an authoritative resnapshot for `watch_id` on the next drain,
    /// regardless of pending mutation state (explicit user refresh).
    ForceResnapshot(WatchId),
    /// Stop the compactor thread.
    Shutdown,
}

/// What the compactor decided for one watch when its batch is ready to
/// flush.
#[derive(Debug, Clone)]
pub enum CompactedBatch {
    /// A bounded, precise set of child-path mutations.
    Targeted {
        watch_id: WatchId,
        changed: Vec<PathBuf>,
        removed: Vec<PathBuf>,
    },
    /// Overflow (raw-lane-full or too-many-pending-mutations) or an
    /// unresolved event: the caller must resnapshot this watch
    /// authoritatively. Carries a fresh dirty generation so a caller can
    /// tell distinct overflow episodes apart.
    Dirty { watch_id: WatchId, generation: u64 },
}

/// Bounded sender handle callbacks (platform `notify` closures) hold. Cloning
/// is cheap (an `Arc`-backed `crossbeam_channel::Sender` clone).
#[derive(Clone)]
pub struct RawLaneSender {
    tx: Sender<RawMutation>,
    overflow: Arc<OverflowTracker>,
}

impl RawLaneSender {
    /// Non-blocking push. Never blocks the calling (platform callback)
    /// thread: a full lane marks the affected watch dirty and drops the
    /// event instead of waiting for capacity.
    pub fn push(&self, mutation: RawMutation) {
        let watch_id = mutation.watch_id;
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
            self.tx.try_send(mutation)
        {
            self.overflow.mark_dirty(watch_id);
        }
    }
}

#[derive(Default)]
struct OverflowTracker {
    dirty: Mutex<std::collections::HashSet<WatchId>>,
}

impl OverflowTracker {
    fn mark_dirty(&self, watch_id: WatchId) {
        self.dirty
            .lock()
            .expect("overflow tracker lock")
            .insert(watch_id);
    }

    fn take_dirty(&self) -> std::collections::HashSet<WatchId> {
        std::mem::take(&mut *self.dirty.lock().expect("overflow tracker lock"))
    }
}

struct PendingWatchState {
    changed: HashMap<PathBuf, ()>,
    removed: HashMap<PathBuf, ()>,
    unresolved: bool,
    oldest_event_at: Instant,
    newest_event_at: Instant,
}

impl PendingWatchState {
    fn new(now: Instant) -> Self {
        Self {
            changed: HashMap::new(),
            removed: HashMap::new(),
            unresolved: false,
            oldest_event_at: now,
            newest_event_at: now,
        }
    }

    fn pending_count(&self) -> usize {
        self.changed.len() + self.removed.len()
    }

    fn record(&mut self, mutation: &RawMutation, now: Instant) {
        self.newest_event_at = now;
        match mutation.kind {
            MutationKind::Changed => {
                self.removed.remove(&mutation.child_path);
                self.changed.insert(mutation.child_path.clone(), ());
            }
            MutationKind::Removed => {
                self.changed.remove(&mutation.child_path);
                self.removed.insert(mutation.child_path.clone(), ());
            }
            MutationKind::Unresolved => {
                self.unresolved = true;
            }
        }
    }

    fn ready_to_flush(&self, now: Instant) -> bool {
        self.unresolved
            || self.pending_count() > MAX_PENDING_MUTATIONS_PER_WATCH
            || now.duration_since(self.newest_event_at) >= QUIET_WINDOW
            || now.duration_since(self.oldest_event_at) >= MAX_BATCH_AGE
    }
}

/// The bounded watch coordinator: owns the raw/control lanes and a
/// background compactor thread that emits [`CompactedBatch`] values through
/// `on_batch`. Constructing this does not itself register any platform
/// watcher — callers push [`RawMutation`]s via [`WatchCoordinator::raw_sender`]
/// from their own `notify` callback.
pub struct WatchCoordinator {
    raw_tx: Sender<RawMutation>,
    control_tx: Sender<ControlMessage>,
    overflow: Arc<OverflowTracker>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl WatchCoordinator {
    /// Spawns the compactor thread. `on_batch` is invoked from the
    /// compactor thread whenever a watch's pending state is ready to flush;
    /// it must not block on further coordinator work.
    pub fn spawn(on_batch: Arc<dyn Fn(CompactedBatch) + Send + Sync>) -> Self {
        let (raw_tx, raw_rx) = bounded::<RawMutation>(RAW_LANE_CAPACITY);
        let (control_tx, control_rx) = bounded::<ControlMessage>(CONTROL_LANE_CAPACITY);
        let overflow = Arc::new(OverflowTracker::default());
        let overflow_for_thread = Arc::clone(&overflow);

        let thread = std::thread::Builder::new()
            .name("watch-coordinator".to_string())
            .spawn(move || {
                run_compactor(raw_rx, control_rx, overflow_for_thread, on_batch);
            })
            .expect("failed to spawn watch coordinator thread");

        Self {
            raw_tx,
            control_tx,
            overflow,
            thread: Mutex::new(Some(thread)),
        }
    }

    /// A cloneable, non-blocking sender handle for platform callbacks to
    /// push [`RawMutation`]s through.
    pub fn raw_sender(&self) -> RawLaneSender {
        RawLaneSender {
            tx: self.raw_tx.clone(),
            overflow: Arc::clone(&self.overflow),
        }
    }

    /// Sends a control message. Control messages use their own bounded lane
    /// and are drained ahead of any pending raw-mutation batch, so this
    /// completes promptly even while the raw lane is saturated. Returns
    /// `false` if the control lane itself is full (should never happen in
    /// practice at 64 capacity, but callers get an explicit signal rather
    /// than a silent block) or the coordinator has shut down.
    pub fn send_control(&self, message: ControlMessage) -> bool {
        self.control_tx.try_send(message).is_ok()
    }

    /// Shuts the compactor thread down deterministically. Safe to call more
    /// than once.
    pub fn shutdown(&self) {
        let mut guard = self.thread.lock().expect("watch coordinator thread lock");
        let Some(handle) = guard.take() else {
            return;
        };
        // Shutdown travels the control lane so it is never stuck behind
        // queued mutation batches, matching the "control messages complete
        // promptly under mutation saturation" acceptance criterion.
        let _ = self.control_tx.send(ControlMessage::Shutdown);
        let _ = handle.join();
    }
}

impl Drop for WatchCoordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Compactor thread body: repeatedly drains whatever is available (control
/// messages first, then raw mutations), updates per-watch pending state, and
/// checks every watch with pending state for flush-readiness on each wake
/// (either a new message arriving or a bounded poll tick so quiet-window/
/// max-age deadlines are honored even without new traffic).
fn run_compactor(
    raw_rx: Receiver<RawMutation>,
    control_rx: Receiver<ControlMessage>,
    overflow: Arc<OverflowTracker>,
    on_batch: Arc<dyn Fn(CompactedBatch) + Send + Sync>,
) {
    let mut pending: HashMap<WatchId, PendingWatchState> = HashMap::new();
    let mut dirty_generation = 1u64;
    // Poll tick bounds how long the thread can block without re-checking
    // flush deadlines; kept well under `QUIET_WINDOW` so a batch that goes
    // quiet is flushed close to on time rather than waiting for the next
    // unrelated message.
    let poll_tick = Duration::from_millis(25);

    loop {
        // Control lane is always drained first and completely, ahead of any
        // raw-mutation processing, so control work can never be starved by
        // mutation volume.
        let mut shutting_down = false;
        while let Ok(message) = control_rx.try_recv() {
            match message {
                ControlMessage::Unwatch(id) => {
                    pending.remove(&id);
                }
                ControlMessage::Replace(id) => {
                    pending.remove(&id);
                }
                ControlMessage::ForceResnapshot(id) => {
                    pending.remove(&id);
                    let generation = dirty_generation;
                    dirty_generation += 1;
                    on_batch(CompactedBatch::Dirty {
                        watch_id: id,
                        generation,
                    });
                }
                ControlMessage::Shutdown => {
                    shutting_down = true;
                }
            }
        }
        if shutting_down {
            return;
        }

        // Overflow-marked watches (raw lane was full when a callback tried
        // to push) become dirty regardless of what detailed state we may
        // still hold for them.
        for watch_id in overflow.take_dirty() {
            pending.remove(&watch_id);
            let generation = dirty_generation;
            dirty_generation += 1;
            on_batch(CompactedBatch::Dirty {
                watch_id,
                generation,
            });
        }

        let now = Instant::now();
        match raw_rx.recv_timeout(poll_tick) {
            Ok(mutation) => {
                let watch_id = mutation.watch_id;
                let state = pending
                    .entry(watch_id)
                    .or_insert_with(|| PendingWatchState::new(now));
                state.record(&mutation, now);

                // Drain everything else immediately available without
                // blocking, so a burst arriving together is compacted in one
                // pass rather than one wake per event.
                while let Ok(next) = raw_rx.try_recv() {
                    let watch_id = next.watch_id;
                    let state = pending
                        .entry(watch_id)
                        .or_insert_with(|| PendingWatchState::new(now));
                    state.record(&next, now);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }

        flush_ready(&mut pending, &mut dirty_generation, &on_batch);
    }
}

fn flush_ready(
    pending: &mut HashMap<WatchId, PendingWatchState>,
    dirty_generation: &mut u64,
    on_batch: &Arc<dyn Fn(CompactedBatch) + Send + Sync>,
) {
    let now = Instant::now();
    let ready: Vec<WatchId> = pending
        .iter()
        .filter(|(_, state)| state.ready_to_flush(now))
        .map(|(id, _)| *id)
        .collect();

    for watch_id in ready {
        let Some(state) = pending.remove(&watch_id) else {
            continue;
        };

        if state.unresolved || state.pending_count() > MAX_PENDING_MUTATIONS_PER_WATCH {
            let generation = *dirty_generation;
            *dirty_generation += 1;
            on_batch(CompactedBatch::Dirty {
                watch_id,
                generation,
            });
            continue;
        }

        if state.changed.is_empty() && state.removed.is_empty() {
            continue;
        }

        on_batch(CompactedBatch::Targeted {
            watch_id,
            changed: state.changed.into_keys().collect(),
            removed: state.removed.into_keys().collect(),
        });
    }
}
