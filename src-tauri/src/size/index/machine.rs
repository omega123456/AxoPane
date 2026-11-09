use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use crossbeam_channel::{Receiver, Sender};

use super::persist::{self, ChangeEvent, ReplayOutcome, Snapshot, SnapshotMeta};
use super::platform::{self, IndexPlatform, PowerSignal, RawEvent, StreamHandle, StreamRequest};
use super::tree::{DirectoryListing, FolderSizeTree};
use crate::size::fast_walk::{self, DirRecord, Qos};
use crate::size::manual::ManualSizeError;
use crate::size::{SizeRequest, SizeSource, SizeStateKind, SizeUpdate};

use super::lanes::{BoostJob, ScanJob, WalkOutcome};
use super::service::{
    unix_secs, unknown, update_from_total, BoostRequest, Clock, IndexOptions, Shared, SizeEmitter,
    StatusEmitter, WalkTuning,
};
use super::subscriptions::SubscribedFolder;

struct RescanStart {
    identity: SnapshotMeta,
    current_id: u64,
}

pub(super) enum Message {
    Events(Vec<RawEvent>),
    Power(PowerSignal),
    Records {
        records: Vec<DirRecord>,
        full: bool,
    },
    ScanDone {
        job: ScanJob,
        outcome: WalkOutcome,
    },
    BoostDone {
        job: BoostJob,
        outcome: WalkOutcome,
    },
    Request {
        request: SizeRequest,
        emitter: SizeEmitter,
    },
    Subscribe {
        pane: String,
        folders: Vec<SubscribedFolder>,
    },
    Visited(PathBuf),
    Focus(bool),
    Flush(Sender<()>),
    Shutdown,
}

pub(super) struct FullScan {
    /// The event ID captured before the scan.
    pub(super) captured_id: u64,
    pub(super) remaining: usize,
}

pub(super) struct CatchUp {
    pub(super) saved: SnapshotMeta,
    pub(super) current_id: u64,
    pub(super) events: Vec<ChangeEvent>,
}

pub(super) enum Phase {
    Booting,
    ReplayingHistory(CatchUp),
    Live,
}

pub(super) enum AbsorbedEvents {
    Buffered,
    HistoryReplayed(CatchUp),
    LiveBatch(Vec<ChangeEvent>),
}

impl Phase {
    pub(super) fn replaying_history(saved: SnapshotMeta, current_id: u64) -> Self {
        Self::ReplayingHistory(CatchUp {
            saved,
            current_id,
            events: Vec::new(),
        })
    }

    pub(super) fn absorb(&mut self, live: Vec<ChangeEvent>, history_done: bool) -> AbsorbedEvents {
        let Self::ReplayingHistory(catch_up) = self else {
            return AbsorbedEvents::LiveBatch(live);
        };
        catch_up.events.extend(live);
        if !history_done {
            return AbsorbedEvents::Buffered;
        }
        let Self::ReplayingHistory(replayed) = std::mem::replace(self, Self::Live) else {
            return AbsorbedEvents::Buffered;
        };
        AbsorbedEvents::HistoryReplayed(replayed)
    }
}

pub(super) struct Machine {
    pub(super) shared: Arc<Shared>,
    pub(super) sender: Sender<Message>,
    pub(super) platform: Arc<dyn IndexPlatform>,
    pub(super) clock: Clock,
    pub(super) emit_sizes: SizeEmitter,
    pub(super) emit_status: StatusEmitter,
    pub(super) snapshot_path: PathBuf,
    pub(super) stream_excluded: Vec<PathBuf>,
    pub(super) stream: Option<StreamHandle>,
    pub(super) phase: Phase,
    pub(super) meta: SnapshotMeta,
    pub(super) meta_dirty: bool,
    pub(super) last_applied_id: u64,
    pub(super) full: Option<FullScan>,
    pub(super) background_inflight: usize,
    pub(super) items_scanned: u64,
    pub(super) subscriptions: HashMap<String, Vec<SubscribedFolder>>,
    pub(super) sleeping: bool,
    pub(super) low_power: bool,
    pub(super) thermal_pressure: bool,
    pub(super) last_progress_emit: Option<SystemTime>,
}

impl Machine {
    pub(super) fn new(shared: Arc<Shared>, options: IndexOptions, sender: Sender<Message>) -> Self {
        Self {
            shared,
            sender,
            platform: options.platform,
            clock: options.clock,
            emit_sizes: options.emit_sizes,
            emit_status: options.emit_status,
            snapshot_path: options.snapshot_path,
            stream_excluded: options.stream_excluded,
            stream: None,
            phase: Phase::Booting,
            meta: SnapshotMeta::default(),
            meta_dirty: false,
            last_applied_id: 0,
            full: None,
            background_inflight: 0,
            items_scanned: 0,
            subscriptions: HashMap::new(),
            sleeping: false,
            low_power: false,
            thermal_pressure: false,
            last_progress_emit: None,
        }
    }

    pub(super) fn run(mut self, receiver: &Receiver<Message>) {
        (self.emit_status)(
            self.shared
                .status
                .lock()
                .expect("index status lock")
                .clone(),
        );
        self.boot();
        self.after_change();
        while let Ok(message) = receiver.recv() {
            match message {
                Message::Shutdown => break,
                message => self.handle(message),
            }
            self.after_change();
        }
    }

    pub(super) fn boot(&mut self) {
        self.observe_power();
        let since = self.restore_snapshot();
        self.apply_privacy();
        self.shared.booted.store(true, Ordering::Release);

        self.start_stream(since);
        if self.stream.is_none() {
            self.shared.view_mut().available = false;
            self.shared.lanes().background.clear();
            return;
        }
        if since.is_none() || self.weekly_due() {
            self.start_full_scan();
            return;
        }
        let tops = self.shared.view().tree.unscanned_tops();
        for top in tops {
            self.queue_background(top);
        }
    }

    fn observe_power(&mut self) {
        let power_sender = self.sender.clone();
        let conditions = self.platform.observe_power(Arc::new(move |signal| {
            let _ = power_sender.send(Message::Power(signal));
        }));
        self.power(conditions);
    }

    /// Returns the event ID history replays from, or `None` when the index
    /// must rescan.
    fn restore_snapshot(&mut self) -> Option<u64> {
        // FSEvents history for user data lives on the Data volume; on
        // macOS 10.15–13 `/` itself is the sealed system volume.
        let identity = SnapshotMeta {
            volume_uuid: self
                .platform
                .volume_uuid(Path::new(platform::DATA_VOLUME_PREFIX))
                .unwrap_or_default(),
            os_build: self.platform.os_build().unwrap_or_default(),
            ..SnapshotMeta::default()
        };
        let current_id = self.platform.current_event_id();
        let snapshot = persist::load(&self.snapshot_path);
        self.restore_access(snapshot.as_ref());
        match snapshot {
            Some(snapshot) if is_resumable(&snapshot.meta, &identity, current_id) => {
                Some(self.resume_from(snapshot, current_id))
            }
            stale => {
                self.rescan_from(
                    stale,
                    RescanStart {
                        identity,
                        current_id,
                    },
                );
                None
            }
        }
    }

    fn restore_access(&mut self, snapshot: Option<&Snapshot>) {
        let mut view = self.shared.view_mut();
        view.full_disk_access = self.platform.full_disk_access();
        if let Some(snapshot) = snapshot {
            view.visited
                .extend(snapshot.meta.visited_roots.iter().cloned());
        }
    }

    fn resume_from(&mut self, snapshot: Snapshot, current_id: u64) -> u64 {
        let saved = snapshot.meta.clone();
        self.load_tree(snapshot.tree);
        self.meta = snapshot.meta;
        self.last_applied_id = saved.event_id;
        self.phase = Phase::replaying_history(saved, current_id);
        self.meta.event_id
    }

    /// Missing, corrupt, foreign, from another OS build or ahead: rescan,
    /// keeping any old totals visible meanwhile.
    fn rescan_from(&mut self, stale: Option<Snapshot>, start: RescanStart) {
        let RescanStart {
            identity,
            current_id,
        } = start;
        if let Some(snapshot) = stale {
            self.load_tree(snapshot.tree);
        }
        self.meta = identity;
        self.last_applied_id = current_id;
        self.phase = Phase::Live;
    }

    fn start_stream(&mut self, since: Option<u64>) {
        let sender = self.sender.clone();
        let request = StreamRequest {
            roots: &self.shared.roots,
            excluded: &self.stream_excluded,
            since,
        };
        self.stream = self.platform.start_stream(
            request,
            Arc::new(move |events| {
                let _ = sender.send(Message::Events(events));
            }),
        );
    }

    pub(super) fn load_tree(&mut self, mut tree: FolderSizeTree) {
        for root in &self.shared.roots {
            tree.add_root(root);
        }
        self.shared.view_mut().tree = tree;
    }

    pub(super) fn handle(&mut self, message: Message) {
        match message {
            Message::Events(events) => self.events(events),
            Message::Power(signal) => self.power(signal),
            Message::Records { records, full } => self.records(records, full),
            Message::ScanDone { job, outcome } => self.scan_done(job, outcome),
            Message::BoostDone { job, outcome } => self.boost_done(job, outcome),
            Message::Request { request, emitter } => self.shared.request(request, emitter),
            Message::Subscribe { pane, folders } => self.subscribe(pane, folders),
            Message::Visited(root) => self.visited(root),
            Message::Focus(true) => self.focused(),
            Message::Focus(false) => self.save_snapshot(),
            Message::Flush(ack) => {
                self.save_snapshot();
                let _ = ack.send(());
            }
            Message::Shutdown => {}
        }
    }

    pub(super) fn events(&mut self, raw: Vec<RawEvent>) {
        let mut batch = RawEventBatch::default();
        for event in raw {
            batch.add(event, &self.shared);
        }
        let max_id = batch.max_id;
        match self.phase.absorb(batch.live, batch.history_done) {
            AbsorbedEvents::Buffered => {}
            AbsorbedEvents::HistoryReplayed(replayed) => {
                self.finish_history_replay(replayed, max_id);
            }
            AbsorbedEvents::LiveBatch(live) => {
                self.apply_plan(persist::plan_batch(&live));
                self.last_applied_id = self.last_applied_id.max(max_id);
            }
        }
    }

    pub(super) fn finish_history_replay(&mut self, replayed: CatchUp, max_id: u64) {
        let current = persist::CurrentVolume {
            uuid: &self.meta.volume_uuid,
            os_build: &self.meta.os_build,
            event_id: replayed.current_id,
        };
        let plan = persist::plan_catch_up(&replayed.saved, current, &replayed.events);
        self.apply_plan(plan);
        self.last_applied_id = self.last_applied_id.max(max_id).max(replayed.current_id);
        self.save_snapshot();
    }

    pub(super) fn apply_plan(&mut self, outcome: ReplayOutcome) {
        match outcome {
            ReplayOutcome::UpToDate => {}
            ReplayOutcome::FullRescan => self.start_full_scan(),
            ReplayOutcome::Apply { reread, rescan } => self.apply_changes(reread, rescan),
        }
    }

    fn apply_changes(&mut self, reread: Vec<PathBuf>, rescan: Vec<PathBuf>) {
        for dir in reread {
            self.reread(&dir);
        }
        for root in rescan {
            self.queue_background(root);
        }
    }

    /// Re-reads one reported directory. Directories a scan has
    /// not reached yet, and unvisited protected ones, are left alone.
    pub(super) fn reread(&mut self, dir: &Path) {
        if !self.can_reread(dir) {
            return;
        }
        let tuning = WalkTuning {
            qos: Qos::UserInitiated,
            threads: 1,
        };
        let options = self
            .shared
            .walk_options(Arc::new(AtomicBool::new(false)), tuning);
        let added = match fast_walk::list_dir(dir, &options) {
            Ok(listing) => self.shared.view_mut().tree.apply_listing(
                dir,
                DirectoryListing {
                    direct_bytes: listing.direct_bytes,
                    subdirs: &listing.subdirs,
                    denied: listing.denied,
                },
            ),
            Err(ManualSizeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                self.prune(dir);
                None
            }
            Err(error) => {
                log::warn!("size index: re-read of {} failed: {error}", dir.display());
                None
            }
        };
        self.reskip_protected();
        let subscribed = self.subscribed();
        for path in added.unwrap_or_default() {
            self.schedule_added_dir(path, &subscribed);
        }
    }

    fn can_reread(&self, dir: &Path) -> bool {
        let view = self.shared.view();
        let is_scanned = view.tree.total(dir).is_some_and(|total| total.scanned);
        is_scanned && !self.shared.privacy_blocked(&view, dir)
    }

    fn schedule_added_dir(&mut self, path: PathBuf, subscribed: &[SubscribedFolder]) {
        if self.shared.privacy_blocked(&self.shared.view(), &path) {
            return;
        }
        let is_subscribed = subscribed.iter().any(|folder| folder.normalized == path);
        if !is_subscribed {
            self.queue_background(path);
            return;
        }
        let display = path.to_string_lossy().into_owned();
        self.shared.queue_boosts(BoostRequest {
            paths: vec![display],
            refresh: false,
            waiter: None,
        });
    }

    pub(super) fn prune(&mut self, dir: &Path) {
        let is_root = self.shared.roots.iter().any(|root| root == dir);
        if !is_root {
            self.shared.view_mut().tree.prune(dir);
        }
    }

    pub(super) fn start_full_scan(&mut self) {
        if self.full.is_some() {
            return;
        }
        self.items_scanned = 0;
        self.full = Some(FullScan {
            captured_id: self.platform.current_event_id(),
            remaining: 0,
        });
        let queued_roots = self
            .shared
            .roots
            .clone()
            .into_iter()
            .map(|root| self.shared.queue_background(root, true))
            .filter(|queued| *queued)
            .count();
        self.background_inflight += queued_roots;
        if let Some(full) = &mut self.full {
            full.remaining += queued_roots;
        }
        if self.full.as_ref().is_some_and(|full| full.remaining == 0) {
            self.finish_full_scan();
        }
    }

    pub(super) fn queue_background(&mut self, root: PathBuf) {
        if self.shared.queue_background(root, false) {
            self.background_inflight += 1;
        }
    }

    pub(super) fn records(&mut self, records: Vec<DirRecord>, full: bool) {
        self.merge_records(&records);
        self.reskip_protected();
        if full {
            // Items are folders plus their files.
            self.items_scanned += records
                .iter()
                .map(|record| 1 + record.file_count)
                .sum::<u64>();
        }
    }

    fn merge_records(&self, records: &[DirRecord]) {
        let mut view = self.shared.view_mut();
        for record in records {
            view.tree.merge_record(
                &record.path,
                DirectoryListing {
                    direct_bytes: record.direct_bytes,
                    subdirs: &record.subdirs,
                    denied: record.denied,
                },
            );
        }
    }

    pub(super) fn scan_done(&mut self, job: ScanJob, outcome: WalkOutcome) {
        self.background_inflight = self.background_inflight.saturating_sub(1);
        self.walk_finished(&job.root, &outcome);
        if !job.full {
            return;
        }
        let Some(full) = &mut self.full else {
            return;
        };
        full.remaining = full.remaining.saturating_sub(1);
        if full.remaining == 0 {
            self.finish_full_scan();
        }
    }

    pub(super) fn finish_full_scan(&mut self) {
        if let Some(full) = self.full.take() {
            // Saved as is, even if later events were applied.
            self.meta.event_id = full.captured_id;
            self.last_applied_id = full.captured_id;
            self.meta.last_full_scan_secs = unix_secs((self.clock)());
            self.meta_dirty = true;
            self.save_snapshot();
        }
    }

    pub(super) fn boost_done(&mut self, job: BoostJob, outcome: WalkOutcome) {
        let path = platform::normalize(Path::new(&job.path));
        self.walk_finished(&path, &outcome);
        let update = match &outcome {
            WalkOutcome::Failed(ManualSizeError::Cancelled) => return,
            WalkOutcome::Failed(_) => size_error(&job.path),
            WalkOutcome::Walked | WalkOutcome::Skipped => self.walked_update(&job.path, &path),
        };
        for waiter in job.waiters {
            waiter(vec![update.clone()]);
        }
    }

    fn walked_update(&self, display_path: &str, path: &Path) -> SizeUpdate {
        let view = self.shared.view();
        if self.shared.privacy_blocked(&view, path) {
            return unknown(display_path);
        }
        view.tree.total(path).map_or_else(
            || size_error(display_path),
            |total| update_from_total(display_path, &total),
        )
    }

    pub(super) fn walk_finished(&mut self, root: &Path, outcome: &WalkOutcome) {
        match outcome {
            WalkOutcome::Walked => {
                self.shared.view_mut().tree.prune_unscanned(root);
            }
            WalkOutcome::Failed(ManualSizeError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                self.prune(root);
            }
            WalkOutcome::Skipped | WalkOutcome::Failed(_) => {}
        }
    }
}

fn is_resumable(saved: &SnapshotMeta, identity: &SnapshotMeta, current_id: u64) -> bool {
    saved.volume_uuid == identity.volume_uuid
        && saved.os_build == identity.os_build
        && saved.event_id <= current_id
}

fn size_error(path: &str) -> SizeUpdate {
    SizeUpdate::without_size(path.to_string(), SizeStateKind::Error, SizeSource::Index)
}

#[derive(Default)]
struct RawEventBatch {
    live: Vec<ChangeEvent>,
    history_done: bool,
    max_id: u64,
}

impl RawEventBatch {
    fn add(&mut self, event: RawEvent, shared: &Shared) {
        if event.history_done {
            self.history_done = true;
            return;
        }
        self.max_id = self.max_id.max(event.id);
        let flags = event.flags;
        let path = platform::normalize(&event.path);
        let forces_full_rescan = flags.user_dropped
            || flags.kernel_dropped
            || flags.event_ids_wrapped
            || flags.root_changed;
        // Drops other volumes, devfs and the non-Data parts of
        // `/System/Volumes`; Data-prefixed paths were normalized.
        let should_record = forces_full_rescan || shared.indexes(&path);
        if should_record {
            self.live.push(ChangeEvent { path, flags });
        }
    }
}
