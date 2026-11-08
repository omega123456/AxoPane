//! Bounded watch traffic coordinator.
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
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};

/// Fixed capacity of the raw notification lane. A `try_send` that finds this
/// lane full marks the affected watch dirty rather than blocking or growing
/// without bound.
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

/// Bounds how long the thread can block without re-checking flush deadlines;
/// kept well under `QUIET_WINDOW` so a batch that goes quiet is flushed close
/// to on time rather than waiting for the next unrelated message.
const POLL_TICK: Duration = Duration::from_millis(25);

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
    sender: Sender<RawMutation>,
    overflow: Arc<OverflowTracker>,
}

impl RawLaneSender {
    /// Non-blocking push. Never blocks the calling (platform callback)
    /// thread: a full lane marks the affected watch dirty and drops the
    /// event instead of waiting for capacity.
    pub fn push(&self, mutation: RawMutation) {
        let watch_id = mutation.watch_id;
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
            self.sender.try_send(mutation)
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
    raw_sender: Sender<RawMutation>,
    control_sender: Sender<ControlMessage>,
    overflow: Arc<OverflowTracker>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl WatchCoordinator {
    /// Spawns the compactor thread. `on_batch` is invoked from the
    /// compactor thread whenever a watch's pending state is ready to flush;
    /// it must not block on further coordinator work.
    pub fn spawn(on_batch: Arc<dyn Fn(CompactedBatch) + Send + Sync>) -> Self {
        let (raw_sender, raw_receiver) = bounded::<RawMutation>(RAW_LANE_CAPACITY);
        let (control_sender, control_receiver) = bounded::<ControlMessage>(CONTROL_LANE_CAPACITY);
        let overflow = Arc::new(OverflowTracker::default());
        let overflow_for_thread = Arc::clone(&overflow);

        let thread = std::thread::Builder::new()
            .name("watch-coordinator".to_string())
            .spawn(move || {
                run_compactor(
                    raw_receiver,
                    control_receiver,
                    overflow_for_thread,
                    on_batch,
                );
            })
            .expect("failed to spawn watch coordinator thread");

        Self {
            raw_sender,
            control_sender,
            overflow,
            thread: Mutex::new(Some(thread)),
        }
    }

    /// A cloneable, non-blocking sender handle for platform callbacks to
    /// push [`RawMutation`]s through.
    pub fn raw_sender(&self) -> RawLaneSender {
        RawLaneSender {
            sender: self.raw_sender.clone(),
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
        self.control_sender.try_send(message).is_ok()
    }

    /// Shuts the compactor thread down deterministically. Safe to call more
    /// than once.
    pub fn shutdown(&self) {
        let mut guard = self.thread.lock().expect("watch coordinator thread lock");
        let Some(handle) = guard.take() else {
            return;
        };
        // Shutdown travels the control lane so it is never stuck behind
        // queued mutation batches and completes promptly under mutation
        // saturation.
        let _ = self.control_sender.send(ControlMessage::Shutdown);
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
    raw_receiver: Receiver<RawMutation>,
    control_receiver: Receiver<ControlMessage>,
    overflow: Arc<OverflowTracker>,
    on_batch: Arc<dyn Fn(CompactedBatch) + Send + Sync>,
) {
    let mut compactor = Compactor {
        raw_receiver,
        control_receiver,
        overflow,
        on_batch,
        pending: HashMap::new(),
        dirty_generation: 1,
    };
    while compactor.run_cycle().is_continue() {}
}

struct Compactor {
    raw_receiver: Receiver<RawMutation>,
    control_receiver: Receiver<ControlMessage>,
    overflow: Arc<OverflowTracker>,
    on_batch: Arc<dyn Fn(CompactedBatch) + Send + Sync>,
    pending: HashMap<WatchId, PendingWatchState>,
    dirty_generation: u64,
}

impl Compactor {
    fn run_cycle(&mut self) -> ControlFlow<()> {
        // Control lane is always drained first and completely, ahead of any
        // raw-mutation processing, so control work can never be starved by
        // mutation volume.
        let shutting_down = self.drain_control_lane();
        if shutting_down {
            return ControlFlow::Break(());
        }
        // Overflow-marked watches (raw lane was full when a callback tried
        // to push) become dirty regardless of what detailed state we may
        // still hold for them.
        for watch_id in self.overflow.take_dirty() {
            self.mark_dirty(watch_id);
        }
        self.absorb_raw_mutations()?;
        self.flush_ready();
        ControlFlow::Continue(())
    }

    fn drain_control_lane(&mut self) -> bool {
        let mut shutting_down = false;
        while let Ok(message) = self.control_receiver.try_recv() {
            shutting_down |= matches!(message, ControlMessage::Shutdown);
            self.handle_control(message);
        }
        shutting_down
    }

    fn handle_control(&mut self, message: ControlMessage) {
        match message {
            ControlMessage::Unwatch(id) | ControlMessage::Replace(id) => {
                self.pending.remove(&id);
            }
            ControlMessage::ForceResnapshot(id) => self.mark_dirty(id),
            ControlMessage::Shutdown => {}
        }
    }

    fn mark_dirty(&mut self, watch_id: WatchId) {
        self.pending.remove(&watch_id);
        self.emit_dirty(watch_id);
    }

    fn emit_dirty(&mut self, watch_id: WatchId) {
        let generation = self.dirty_generation;
        self.dirty_generation += 1;
        (self.on_batch)(CompactedBatch::Dirty {
            watch_id,
            generation,
        });
    }

    fn absorb_raw_mutations(&mut self) -> ControlFlow<()> {
        let now = Instant::now();
        match self.raw_receiver.recv_timeout(POLL_TICK) {
            Ok(mutation) => self.record_burst(&mutation, now),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    /// Drains everything else immediately available without blocking, so a
    /// burst arriving together is compacted in one pass rather than one wake
    /// per event.
    fn record_burst(&mut self, first: &RawMutation, now: Instant) {
        self.record(first, now);
        while let Ok(next) = self.raw_receiver.try_recv() {
            self.record(&next, now);
        }
    }

    fn record(&mut self, mutation: &RawMutation, now: Instant) {
        self.pending
            .entry(mutation.watch_id)
            .or_insert_with(|| PendingWatchState::new(now))
            .record(mutation, now);
    }

    fn flush_ready(&mut self) {
        let now = Instant::now();
        let ready: Vec<WatchId> = self
            .pending
            .iter()
            .filter(|(_, state)| state.ready_to_flush(now))
            .map(|(id, _)| *id)
            .collect();
        for watch_id in ready {
            self.flush_watch(watch_id);
        }
    }

    fn flush_watch(&mut self, watch_id: WatchId) {
        let Some(state) = self.pending.remove(&watch_id) else {
            return;
        };
        if state.unresolved || state.pending_count() > MAX_PENDING_MUTATIONS_PER_WATCH {
            self.emit_dirty(watch_id);
            return;
        }
        if state.changed.is_empty() && state.removed.is_empty() {
            return;
        }
        (self.on_batch)(CompactedBatch::Targeted {
            watch_id,
            changed: state.changed.into_keys().collect(),
            removed: state.removed.into_keys().collect(),
        });
    }
}
