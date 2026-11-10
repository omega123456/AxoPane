use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crossbeam_channel::{Receiver, Sender};

use super::compact::{CompactJob, Compactor, FinishedCompaction};
use super::exclusions;
use super::exclusions::plan_reconcile;
use super::grafts::{FinishedWalk, WalkEnd, WalkLogs};
use super::names::writer::RewriteStats;
use super::names::{PackedNames, SavedNames};
use super::persist::{self, ChangeEvent, ReplayOutcome, Snapshot, SnapshotMeta};
use super::platform::{self, IndexPlatform, PowerSignal, RawEvent, StreamHandle, StreamRequest};
use super::snapshot::SaveTrigger;
use super::tree::{DirectoryListing, FolderSizeTree};
use crate::size::fast_walk::{self, DirListing, DirRecord, Qos};
use crate::size::manual::ManualSizeError;
use crate::size::{SizeRequest, SizeSource, SizeStateKind, SizeUpdate};

use super::lanes::{BoostJob, Lane, ScanJob, ScanKind, WalkOutcome};
use super::service::{
    unix_secs, unknown, BoostRequest, Clock, IndexOptions, Shared, SizeEmitter, StatusEmitter,
    View, WalkTuning,
};
use super::subscriptions::SubscribedFolder;

const SCAN_PROGRESS_LOG_STEP: u64 = 250_000;

struct RescanStart {
    identity: SnapshotMeta,
    current_id: u64,
}

struct Resume {
    snapshot: Snapshot,
    current_id: u64,
}

pub(super) struct MachineLinks {
    pub(super) sender: Sender<Message>,
    pub(super) compact_jobs: Sender<CompactJob>,
}

pub(super) struct ExclusionEdit {
    pub(super) added: Vec<PathBuf>,
    pub(super) removed: Vec<PathBuf>,
}

pub(super) enum Message {
    Events(Vec<RawEvent>),
    Power(PowerSignal),
    Records {
        records: Vec<DirRecord>,
        full: bool,
    },
    WalkStarted {
        root: PathBuf,
        lane: Lane,
    },
    Progress(u64),
    ScanTops(Vec<PathBuf>),
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
    Exclusions {
        edit: ExclusionEdit,
        ack: Sender<()>,
    },
    RetryStream(Sender<()>),
    Rebuild(Sender<()>),
    Save(SaveTrigger),
    NamesUnreadable,
    TreeUnreadable,
    Compacted(FinishedCompaction),
    Shutdown,
}

impl Message {
    fn runs_during_compaction(&self) -> bool {
        matches!(
            self,
            Self::Request { .. }
                | Self::Subscribe { .. }
                | Self::Power(_)
                | Self::Progress(_)
                | Self::Flush(_)
                | Self::Compacted(_)
                | Self::Shutdown
        )
    }
}

pub(super) struct FullScan {
    pub(super) captured_id: u64,
    pub(super) remaining: usize,
    pub(super) started: Instant,
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
    pub(super) rescan_after_full: bool,
    pub(super) compactor: Compactor,
    pub(super) walk_logs: WalkLogs,
    pub(super) purge_saved_names: bool,
    pub(super) background_inflight: usize,
    pub(super) items_scanned: u64,
    pub(super) subscriptions: HashMap<String, Vec<SubscribedFolder>>,
    pub(super) sleeping: bool,
    pub(super) low_power: bool,
    pub(super) thermal_pressure: bool,
    pub(super) last_progress_emit: Option<SystemTime>,
}

impl Machine {
    pub(super) fn new(shared: Arc<Shared>, options: IndexOptions, links: MachineLinks) -> Self {
        Self {
            shared,
            sender: links.sender,
            compactor: Compactor::new(links.compact_jobs),
            walk_logs: WalkLogs::default(),
            rescan_after_full: false,
            purge_saved_names: false,
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
                message => self.receive(message),
            }
            self.after_change();
            self.rescan_if_tree_unreadable();
            self.compact_if_overlay_or_segments_are_full();
        }
        self.stop_compaction();
        log::info!("size index: stopped");
    }

    fn receive(&mut self, message: Message) {
        if message.runs_during_compaction() {
            return self.handle(message);
        }
        if let Some(message) = self.compactor.defer(message) {
            self.handle(message);
        }
    }

    pub(super) fn boot(&mut self) {
        self.observe_power();
        let since = self.restore_snapshot();
        self.skip_sequences_on_disk();
        self.sweep_unreferenced_files();
        self.apply_privacy();
        self.reconcile_exclusions();
        self.shared.booted.store(true, Ordering::Release);
        self.start_index(since);
        if self.purge_saved_names {
            self.start_compaction();
        }
    }

    fn can_compact(&self) -> bool {
        let is_live = self.stream.is_some() && !matches!(self.phase, Phase::Booting);
        is_live && self.full.is_none() && !self.compactor.is_running()
    }

    fn compact_if_overlay_or_segments_are_full(&mut self) {
        if !self.can_compact() {
            return;
        }
        let view = self.shared.view();
        let overlay_is_full = view.tree.overlay_stats().exceeds_cap()
            || view.names.overlay_stats().exceeds_threshold();
        drop(view);
        if overlay_is_full || self.compactor.holds_too_many_segments() {
            self.start_compaction();
        }
    }

    fn rescan_if_tree_unreadable(&mut self) {
        if self.shared.view().tree.take_unreported_corruption() {
            self.tree_unreadable();
        }
    }

    pub(super) fn tree_unreadable(&mut self) {
        log::warn!("size index: the folder tree is unreadable, starting a full rescan");
        let deferred = self.compactor.cancel_running();
        self.compactor.segments.clear();
        self.purge_saved_names = false;
        self.load_tree(FolderSizeTree::default());
        self.shared
            .view_mut()
            .names
            .adopt_base(SavedNames::default(), RewriteStats::default());
        self.apply_privacy();
        self.meta_dirty = true;
        self.rescan_after_full = self.full.is_some();
        self.start_full_scan();
        for message in deferred {
            self.handle(message);
        }
    }

    fn start_index(&mut self, since: Option<u64>) {
        self.start_stream(since);
        if self.stream.is_none() {
            log::warn!("size index: the FSEvents stream could not start, index unavailable");
            self.shared.view_mut().available = false;
            self.shared.lanes().background.clear();
            return;
        }
        log::info!("size index: FSEvents stream started");
        self.shared.view_mut().available = true;
        if since.is_none() {
            self.start_full_scan();
            return;
        }
        if self.meta.full_scan_running {
            self.resume_full_scan();
            return;
        }
        if self.weekly_due() {
            self.start_full_scan();
            return;
        }
        let tops = self.shared.view().tree.unscanned_tops();
        for top in tops {
            self.queue_background(top);
        }
    }

    pub(super) fn retry_stream(&mut self) {
        if self.stream.is_some() {
            return;
        }
        log::info!("size index: retrying the FSEvents stream");
        let since = self.refresh_resume_point();
        self.start_index(since);
        if self.stream.is_none() {
            self.republish_status();
        }
    }

    fn rebuild(&mut self) {
        if self.stream.is_none() {
            return;
        }
        log::info!("size index: rebuild requested");
        self.start_full_scan();
    }

    fn refresh_resume_point(&mut self) -> Option<u64> {
        let Phase::ReplayingHistory(catch_up) = &mut self.phase else {
            return None;
        };
        catch_up.current_id = self.platform.current_event_id();
        Some(self.meta.event_id)
    }

    fn reconcile_exclusions(&mut self) {
        let configured = self.shared.exclusions().paths().to_vec();
        let plan = plan_reconcile(&self.meta.exclusions, &configured);
        for path in &plan.prune {
            self.exclude_folder(path);
        }
        if !plan.prune.is_empty() {
            self.purge_saved_names = true;
        }
        for path in &plan.walk {
            self.shared.view_mut().tree.include_subtree(path);
        }
        self.reskip_protected();
        self.record_exclusions();
    }

    pub(super) fn exclusions_changed(&mut self, edit: ExclusionEdit) {
        log::info!(
            "size index: exclusions changed, {} added, {} removed",
            edit.added.len(),
            edit.removed.len()
        );
        for path in &edit.added {
            self.cancel_walks_within(path);
            self.exclude_folder(path);
        }
        if !edit.added.is_empty() {
            self.purge_saved_names = true;
            self.meta_dirty = true;
        }
        for path in &edit.removed {
            self.include_folder(path);
        }
        self.record_exclusions();
    }

    fn exclude_folder(&self, path: &Path) {
        self.shared.view_mut().exclude_folder(path);
    }

    fn include_folder(&mut self, path: &Path) {
        let included = self.shared.view_mut().tree.include_subtree(path);
        self.reskip_protected();
        let walk_due = included
            && self.stream.is_some()
            && !self.shared.privacy_blocked(&self.shared.view(), path);
        if walk_due {
            self.queue_background(path.to_path_buf());
        }
    }

    fn cancel_walks_within(&mut self, excluded: &Path) {
        let mut lanes = self.shared.lanes();
        let queued_before = lanes.background.len();
        lanes
            .background
            .retain(|job| !exclusions::is_within(&job.root, excluded));
        let dropped = queued_before - lanes.background.len();
        lanes.cancel_running_within(excluded);
        drop(lanes);
        self.background_inflight = self.background_inflight.saturating_sub(dropped);
    }

    fn record_exclusions(&mut self) {
        let current = self.shared.exclusions().paths().to_vec();
        if self.meta.exclusions != current {
            self.meta.exclusions = current;
            self.meta_dirty = true;
        }
    }

    pub(super) fn reskip_excluded(&self) {
        let excluded = self.shared.exclusions().paths().to_vec();
        let mut view = self.shared.view_mut();
        for path in &excluded {
            view.exclude_folder(path);
        }
    }

    fn observe_power(&mut self) {
        let power_sender = self.sender.clone();
        let conditions = self.platform.observe_power(Arc::new(move |signal| {
            let _ = power_sender.send(Message::Power(signal));
        }));
        self.power(conditions);
    }

    fn restore_snapshot(&mut self) -> Option<u64> {
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
        self.discard_legacy_files();
        let start = RescanStart {
            identity,
            current_id,
        };
        match snapshot {
            Some(snapshot) if self.resumes(&snapshot.meta, &start) => {
                log::info!(
                    "size index: resuming from saved snapshot ({} folders, event {})",
                    snapshot.tree.node_count(),
                    snapshot.meta.event_id
                );
                Some(self.resume_from(Resume {
                    snapshot,
                    current_id,
                }))
            }
            stale => {
                log_rescan_reason(stale.as_ref(), self.shared.collect_file_names);
                self.rescan_from(stale, start);
                None
            }
        }
    }

    fn resumes(&self, saved: &SnapshotMeta, start: &RescanStart) -> bool {
        let names_missing = self.shared.collect_file_names && !saved.has_file_names;
        !names_missing && is_resumable(saved, &start.identity, start.current_id)
    }

    fn restore_access(&mut self, snapshot: Option<&Snapshot>) {
        let mut view = self.shared.view_mut();
        view.full_disk_access = self.platform.full_disk_access();
        if let Some(snapshot) = snapshot {
            view.visited
                .extend(snapshot.meta.visited_roots.iter().cloned());
        }
    }

    fn discard_legacy_files(&self) {
        if let Err(error) = self.shared.view().names.remove_legacy_files() {
            log::warn!("size index: the old file-name files could not be removed: {error}");
        }
    }

    fn adopt_snapshot(&mut self, snapshot: Snapshot) -> SnapshotMeta {
        let Snapshot {
            meta,
            segments,
            tree,
            names,
        } = snapshot;
        self.load_tree(tree);
        self.compactor.segments = segments;
        let keeps_saved_names = self.shared.collect_file_names && meta.has_file_names;
        if keeps_saved_names {
            self.shared.view_mut().names.restore(names);
        }
        let stopped_collecting_names = meta.has_file_names && !self.shared.collect_file_names;
        self.purge_saved_names = meta.names_purge_due || stopped_collecting_names;
        meta
    }

    fn resume_from(&mut self, resume: Resume) -> u64 {
        let Resume {
            snapshot,
            current_id,
        } = resume;
        let saved = self.adopt_snapshot(snapshot);
        self.meta = saved.clone();
        self.last_applied_id = saved.event_id;
        self.phase = Phase::replaying_history(saved, current_id);
        self.meta.event_id
    }

    fn rescan_from(&mut self, stale: Option<Snapshot>, start: RescanStart) {
        let RescanStart {
            identity,
            current_id,
        } = start;
        let mut identity = identity;
        if let Some(snapshot) = stale {
            identity.exclusions = self.adopt_snapshot(snapshot).exclusions;
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
        self.shared.view_mut().replace_tree(tree);
    }

    pub(super) fn handle(&mut self, message: Message) {
        match message {
            Message::Events(events) => self.events(events),
            Message::Power(signal) => self.power(signal),
            Message::Records { records, full } => self.records(records, full),
            Message::WalkStarted { root, lane } => self.walk_started(root, lane),
            Message::Progress(items) => self.count_scanned(items),
            Message::ScanTops(tops) => {
                self.queue_scan_tops(tops);
            }
            Message::ScanDone { job, outcome } => self.scan_done(job, outcome),
            Message::BoostDone { job, outcome } => self.boost_done(job, outcome),
            Message::Request { request, emitter } => self.shared.request(request, emitter),
            Message::Subscribe { pane, folders } => self.subscribe(pane, folders),
            Message::Visited(root) => self.visited(root),
            Message::Focus(true) => self.focused(),
            Message::Focus(false) => self.save_snapshot(SaveTrigger::Blur),
            Message::Flush(ack) => {
                self.cancel_compaction();
                self.save_snapshot(SaveTrigger::Exit);
                let _ = ack.send(());
            }
            Message::Exclusions { edit, ack } => {
                self.exclusions_changed(edit);
                let _ = ack.send(());
                let _ = self.sender.send(Message::Save(SaveTrigger::ExclusionEdit));
            }
            Message::RetryStream(ack) => {
                self.retry_stream();
                let _ = ack.send(());
            }
            Message::Rebuild(ack) => {
                self.rebuild();
                let _ = ack.send(());
            }
            Message::Save(trigger) => self.save_snapshot(trigger),
            Message::NamesUnreadable => self.names_unreadable(),
            Message::TreeUnreadable => self.rescan_if_tree_unreadable(),
            Message::Compacted(finished) => self.compacted(finished),
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
        log::info!(
            "size index: replayed {} events since event {}",
            replayed.events.len(),
            replayed.saved.event_id
        );
        let current = persist::CurrentVolume {
            uuid: &self.meta.volume_uuid,
            os_build: &self.meta.os_build,
            event_id: replayed.current_id,
        };
        let plan = persist::plan_catch_up(&replayed.saved, current, &replayed.events);
        self.apply_plan(plan);
        self.last_applied_id = self.last_applied_id.max(max_id).max(replayed.current_id);
        self.save_snapshot(SaveTrigger::CatchUp);
    }

    pub(super) fn apply_plan(&mut self, outcome: ReplayOutcome) {
        match outcome {
            ReplayOutcome::UpToDate => {}
            ReplayOutcome::FullRescan => {
                log::info!("size index: event history needs a full rescan");
                self.start_full_scan();
            }
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

    pub(super) fn reread(&mut self, dir: &Path) {
        self.note_reread(dir);
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
            Ok(listing) => self.shared.view_mut().apply_listing(dir, listing),
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
        self.reskip_excluded();
        let subscribed = self.subscribed();
        for path in added.unwrap_or_default() {
            self.schedule_added_dir(path, &subscribed);
        }
    }

    fn can_reread(&self, dir: &Path) -> bool {
        let view = self.shared.view();
        let is_scanned = view.tree.total(dir).is_some_and(|total| total.scanned);
        is_scanned && !self.shared.privacy_blocked(&view, dir) && !self.shared.user_excludes(dir)
    }

    fn schedule_added_dir(&mut self, path: PathBuf, subscribed: &[SubscribedFolder]) {
        if self.shared.privacy_blocked(&self.shared.view(), &path) {
            return;
        }
        if self.shared.user_excludes(&path) {
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
            self.shared.view_mut().prune_folder(dir);
        }
    }

    pub(super) fn names_unreadable(&mut self) {
        log::warn!("size index: file names are unreadable, starting a full rescan");
        self.shared.view_mut().names.fail_and_discard();
        self.start_full_scan();
    }

    pub(super) fn queue_background(&mut self, root: PathBuf) {
        let job = ScanJob {
            root,
            kind: ScanKind::Update,
        };
        if self.shared.queue_background(job) {
            self.background_inflight += 1;
        }
    }

    pub(super) fn records(&mut self, records: Vec<DirRecord>, full: bool) {
        let scanned_items = records
            .iter()
            .map(|record| 1 + record.file_count)
            .sum::<u64>();
        self.merge_records(records);
        self.reskip_protected();
        self.reskip_excluded();
        if full {
            self.count_scanned(scanned_items);
        }
    }

    pub(super) fn log_scan_progress(&self, scanned_before: u64) {
        let step_before = scanned_before / SCAN_PROGRESS_LOG_STEP;
        let step_now = self.items_scanned / SCAN_PROGRESS_LOG_STEP;
        if step_now != step_before {
            log::info!("size index: full scan reached {} items", self.items_scanned);
        }
    }

    fn merge_records(&self, records: Vec<DirRecord>) {
        let kept: Vec<DirRecord> = records
            .into_iter()
            .filter(|record| !self.shared.user_excludes(&record.path))
            .collect();
        let mut view = self.shared.view_mut();
        for record in kept {
            view.merge_record(record);
        }
    }

    pub(super) fn scan_done(&mut self, job: ScanJob, outcome: WalkOutcome) {
        self.background_inflight = self.background_inflight.saturating_sub(1);
        let is_full = job.kind.is_full();
        self.end_walk(FinishedWalk {
            root: &job.root,
            lane: Lane::Background { full: is_full },
            outcome,
        });
        if !is_full {
            return;
        }
        if let Some(full) = &mut self.full {
            full.remaining = full.remaining.saturating_sub(1);
        }
        self.finish_full_scan_if_done();
    }

    pub(super) fn finish_full_scan(&mut self) {
        let Some(full) = self.full.take() else {
            return;
        };
        if std::mem::take(&mut self.rescan_after_full) {
            return self.start_full_scan();
        }
        self.prune_unscanned_roots();
        log::info!(
            "size index: full scan finished, {} items in {}s",
            self.items_scanned,
            full.started.elapsed().as_secs()
        );
        self.meta.event_id = full.captured_id;
        self.last_applied_id = full.captured_id;
        self.meta.last_full_scan_secs = unix_secs((self.clock)());
        self.meta_dirty = true;
        self.save_snapshot(SaveTrigger::ScanComplete);
    }

    fn prune_unscanned_roots(&self) {
        let mut view = self.shared.view_mut();
        for root in &self.shared.roots {
            view.prune_unscanned(root);
        }
    }

    pub(super) fn boost_done(&mut self, job: BoostJob, outcome: WalkOutcome) {
        let path = platform::normalize(Path::new(&job.path));
        let end = self.end_walk(FinishedWalk {
            root: &path,
            lane: Lane::Boost {
                refresh: job.refresh,
            },
            outcome,
        });
        if self.shared.user_excludes(&path) {
            answer_waiters(job.waiters, &unknown(&job.path));
            return;
        }
        let update = match end {
            WalkEnd::Cancelled => return,
            WalkEnd::Failed => size_error(&job.path),
            WalkEnd::Indexed | WalkEnd::Skipped => self.walked_update(&job.path, &path),
        };
        answer_waiters(job.waiters, &update);
    }

    fn walked_update(&self, display_path: &str, path: &Path) -> SizeUpdate {
        let view = self.shared.view();
        if self.shared.privacy_blocked(&view, path) {
            return unknown(display_path);
        }
        view.tree.total(path).map_or_else(
            || size_error(display_path),
            |total| self.shared.update_from_total(display_path, &total),
        )
    }
}

impl View {
    pub(super) fn replace_tree(&mut self, tree: FolderSizeTree) {
        self.tree = tree;
        self.generation += 1;
    }

    fn merge_record(&mut self, record: DirRecord) {
        let DirRecord {
            path,
            direct_bytes,
            subdirs,
            denied,
            file_names,
            ..
        } = record;
        let listing = DirectoryListing {
            direct_bytes,
            subdirs: &subdirs,
            denied,
        };
        let slot = self.tree.merge_record_slot(&path, listing);
        self.tombstone_removed();
        if let (Some(slot), Some(names)) = (slot, file_names) {
            self.names.set_names(slot, names);
        }
    }

    fn apply_listing(&mut self, dir: &Path, listing: DirListing) -> Option<Vec<PathBuf>> {
        let DirListing {
            direct_bytes,
            subdirs,
            denied,
            file_names,
            ..
        } = listing;
        let tree_listing = DirectoryListing {
            direct_bytes,
            subdirs: &subdirs,
            denied,
        };
        let added = self.tree.apply_listing(dir, tree_listing);
        self.tombstone_removed();
        let slot = added.as_ref().and_then(|_| self.tree.lookup(dir));
        if let (Some(slot), Some(names)) = (slot, file_names) {
            self.names.set_names(slot, names);
        }
        added
    }

    pub(super) fn exclude_folder(&mut self, path: &Path) {
        let Some(excluded) = self.tree.exclude_subtree(path) else {
            return;
        };
        if !excluded.changed {
            return;
        }
        self.tombstone_removed();
        self.clear_names(excluded.slot);
    }

    pub(super) fn prune_folder(&mut self, path: &Path) {
        self.tree.prune(path);
        self.tombstone_removed();
    }

    pub(super) fn prune_unscanned(&mut self, path: &Path) {
        self.tree.prune_unscanned(path);
        self.tombstone_removed();
    }

    fn clear_names(&mut self, slot: u32) {
        if self.keeps_file_names {
            self.names.set_names(slot, PackedNames::default());
        }
    }

    pub(super) fn tombstone_removed(&mut self) {
        let removed_slots = self.tree.take_removed_slots();
        let removed_ranges = self.tree.take_removed_ranges();
        if !self.keeps_file_names {
            return;
        }
        for slot in removed_slots {
            self.names.tombstone(slot);
        }
        for slots in removed_ranges {
            self.names.tombstone_range(slots);
        }
    }
}

fn log_rescan_reason(stale: Option<&Snapshot>, collects_file_names: bool) {
    let reason = rescan_reason(stale, collects_file_names);
    log::info!("size index: a full scan is needed, {reason}");
}

fn rescan_reason(stale: Option<&Snapshot>, collects_file_names: bool) -> &'static str {
    let Some(stale) = stale else {
        return "no usable snapshot";
    };
    let names_missing = collects_file_names && !stale.meta.has_file_names;
    if names_missing {
        return "the saved index holds no file names";
    }
    "the snapshot is not resumable (volume, OS build or event history changed)"
}

fn is_resumable(saved: &SnapshotMeta, identity: &SnapshotMeta, current_id: u64) -> bool {
    saved.volume_uuid == identity.volume_uuid
        && saved.os_build == identity.os_build
        && saved.event_id <= current_id
}

fn answer_waiters(waiters: Vec<SizeEmitter>, update: &SizeUpdate) {
    for waiter in waiters {
        waiter(vec![update.clone()]);
    }
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
        let should_record = forces_full_rescan || shared.indexes(&path);
        if should_record {
            self.live.push(ChangeEvent { path, flags });
        }
    }
}
