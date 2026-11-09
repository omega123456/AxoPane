use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TrySendError};

pub const RAW_LANE_CAPACITY: usize = 4096;

pub const CONTROL_LANE_CAPACITY: usize = 64;

pub const MAX_PENDING_MUTATIONS_PER_WATCH: usize = 2048;

pub const QUIET_WINDOW: Duration = Duration::from_millis(150);

pub const MAX_BATCH_AGE: Duration = Duration::from_millis(500);

const POLL_TICK: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WatchId(pub u64);

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
    Unresolved,
}

#[derive(Debug, Clone)]
pub enum ControlMessage {
    Unwatch(WatchId),
    Replace(WatchId),
    ForceResnapshot(WatchId),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum CompactedBatch {
    Targeted {
        watch_id: WatchId,
        changed: Vec<PathBuf>,
        removed: Vec<PathBuf>,
    },
    Dirty {
        watch_id: WatchId,
        generation: u64,
    },
}

#[derive(Clone)]
pub struct RawLaneSender {
    sender: Sender<RawMutation>,
    overflow: Arc<OverflowTracker>,
}

impl RawLaneSender {
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

    fn has_no_mutations(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }

    fn needs_full_refresh(&self) -> bool {
        self.unresolved || self.pending_count() > MAX_PENDING_MUTATIONS_PER_WATCH
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

pub struct WatchCoordinator {
    raw_sender: Sender<RawMutation>,
    control_sender: Sender<ControlMessage>,
    overflow: Arc<OverflowTracker>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl WatchCoordinator {
    pub fn spawn(on_batch: Arc<dyn Fn(CompactedBatch) + Send + Sync>) -> Self {
        let (raw_sender, raw_receiver) = bounded::<RawMutation>(RAW_LANE_CAPACITY);
        let (control_sender, control_receiver) = bounded::<ControlMessage>(CONTROL_LANE_CAPACITY);
        let overflow = Arc::new(OverflowTracker::default());
        let compactor = Compactor {
            raw_receiver,
            control_receiver,
            overflow: Arc::clone(&overflow),
            on_batch,
            pending: HashMap::new(),
            dirty_generation: 1,
        };

        let thread = std::thread::Builder::new()
            .name("watch-coordinator".to_string())
            .spawn(move || compactor.run())
            .expect("failed to spawn watch coordinator thread");

        Self {
            raw_sender,
            control_sender,
            overflow,
            thread: Mutex::new(Some(thread)),
        }
    }

    pub fn raw_sender(&self) -> RawLaneSender {
        RawLaneSender {
            sender: self.raw_sender.clone(),
            overflow: Arc::clone(&self.overflow),
        }
    }

    pub fn send_control(&self, message: ControlMessage) -> bool {
        self.control_sender.try_send(message).is_ok()
    }

    pub fn shutdown(&self) {
        let mut guard = self.thread.lock().expect("watch coordinator thread lock");
        let Some(handle) = guard.take() else {
            return;
        };
        let _ = self.control_sender.send(ControlMessage::Shutdown);
        let _ = handle.join();
    }
}

impl Drop for WatchCoordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
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
    fn run(mut self) {
        while self.run_cycle().is_continue() {}
    }

    fn run_cycle(&mut self) -> ControlFlow<()> {
        let shutting_down = self.drain_control_lane();
        if shutting_down {
            return ControlFlow::Break(());
        }
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
        if state.needs_full_refresh() {
            self.emit_dirty(watch_id);
            return;
        }
        if state.has_no_mutations() {
            return;
        }
        (self.on_batch)(CompactedBatch::Targeted {
            watch_id,
            changed: state.changed.into_keys().collect(),
            removed: state.removed.into_keys().collect(),
        });
    }
}
