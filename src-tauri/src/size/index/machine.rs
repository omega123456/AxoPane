use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crossbeam_channel::{Receiver, Sender};

use super::exclusions;
use super::exclusions::plan_reconcile;
use super::names::file::{remove_if_present, NamesHeader};
use super::names::{PackedNames, PairExpectation};
use super::persist::{self, ChangeEvent, ReplayOutcome, Snapshot, SnapshotMeta};
use super::platform::{self, IndexPlatform, PowerSignal, RawEvent, StreamHandle, StreamRequest};
use super::snapshot::SaveTrigger;
use super::tree::{DirectoryListing, FolderSizeTree};
use crate::size::fast_walk::{self, DirListing, DirRecord, Qos};
use crate::size::manual::ManualSizeError;
use crate::size::{SizeRequest, SizeSource, SizeStateKind, SizeUpdate};

use super::lanes::{BoostJob, ScanJob, WalkOutcome};
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
    names_event_id: u64,
}

pub(super) struct ExclusionEdit {
    pub(super) added: Vec<PathBuf>,
    pub(super) removed: Vec<PathBuf>,
}

pub(super) struct FlushRequest {
    pub(super) ack: Sender<()>,
    pub(super) deadline: Instant,
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
    Flush(FlushRequest),
    Exclusions {
        edit: ExclusionEdit,
        ack: Sender<()>,
    },
    RetryStream(Sender<()>),
    Rebuild(Sender<()>),
    Save(SaveTrigger),
    NamesUnreadable,
    Shutdown,
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
        log::info!("size index: stopped");
    }

    pub(super) fn boot(&mut self) {
        self.observe_power();
        let since = self.restore_snapshot();
        self.apply_privacy();
        self.reconcile_exclusions();
        self.shared.booted.store(true, Ordering::Release);
        self.start_index(since);
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
        if since.is_none() || self.weekly_due() {
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
        self.discard_stale_spill();
        self.discard_saved_names_when_off();
        let names_event_id = snapshot
            .as_ref()
            .and_then(|snapshot| self.names_resume_point(snapshot));
        match (snapshot, names_event_id) {
            (Some(snapshot), Some(names_event_id))
                if is_resumable(&snapshot.meta, &identity, current_id) =>
            {
                log::info!(
                    "size index: resuming from saved snapshot ({} folders, event {})",
                    snapshot.tree.node_count(),
                    snapshot.meta.event_id.min(names_event_id)
                );
                Some(self.resume_from(Resume {
                    snapshot,
                    current_id,
                    names_event_id,
                }))
            }
            (stale, names) => {
                log_rescan_reason(stale.as_ref(), names.is_some());
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

    fn discard_stale_spill(&self) {
        if let Err(error) = self.shared.view().names.remove_stale_spill() {
            log::warn!("size index: stale file-name spill removal failed: {error}");
        }
    }

    fn names_resume_point(&self, snapshot: &Snapshot) -> Option<u64> {
        if !self.shared.collect_file_names {
            return Some(snapshot.meta.event_id);
        }
        self.open_names_pair(snapshot).map(|names| names.event_id)
    }

    fn discard_saved_names_when_off(&self) {
        if self.shared.collect_file_names {
            return;
        }
        let names_path = self.shared.view().names.path().to_path_buf();
        if let Err(error) = remove_if_present(&names_path) {
            log::warn!("size index: saved file names could not be removed: {error}");
        }
    }

    fn unpair_when_file_names_are_off(&mut self) {
        let is_paired = self.meta.pair_token != persist::NO_PAIR;
        let pair_is_obsolete = is_paired && !self.shared.collect_file_names;
        if pair_is_obsolete {
            self.meta.pair_token = persist::NO_PAIR;
            self.meta_dirty = true;
        }
    }

    fn open_names_pair(&self, snapshot: &Snapshot) -> Option<NamesHeader> {
        let imported_slots = snapshot.tree.heap_ids();
        let expected = PairExpectation {
            pair_token: snapshot.meta.pair_token,
            first_slot: imported_slots.start,
            folder_count: imported_slots.len() as u32,
        };
        if expected.pair_token == persist::NO_PAIR {
            return None;
        }
        self.shared.view_mut().names.open_pair(expected)
    }

    fn resume_from(&mut self, resume: Resume) -> u64 {
        let Resume {
            snapshot,
            current_id,
            names_event_id,
        } = resume;
        let mut saved = snapshot.meta;
        saved.event_id = saved.event_id.min(names_event_id);
        self.load_tree(snapshot.tree);
        self.meta = saved.clone();
        self.unpair_when_file_names_are_off();
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
            identity.exclusions = snapshot.meta.exclusions;
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
            Message::Focus(false) => self.save_snapshot(SaveTrigger::Blur),
            Message::Flush(request) => {
                self.save_snapshot(SaveTrigger::Exit(request.deadline));
                let _ = request.ack.send(());
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

    fn begin_spill(&self) {
        if !self.shared.collect_file_names {
            return;
        }
        if let Err(error) = self.shared.view_mut().names.begin_spill() {
            log::warn!("size index: file-name spill could not start: {error}");
            return;
        }
        log::info!("size index: file-name spill started");
    }

    pub(super) fn restart_spill_if_scanning(&self) {
        if self.full.is_some() {
            self.begin_spill();
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
            started: Instant::now(),
        });
        self.begin_spill();
        let queued_roots = self
            .shared
            .roots
            .clone()
            .into_iter()
            .map(|root| self.shared.queue_background(root, true))
            .filter(|queued| *queued)
            .count();
        self.background_inflight += queued_roots;
        log::info!("size index: full scan started with {queued_roots} roots queued");
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
        let scanned_items = records
            .iter()
            .map(|record| 1 + record.file_count)
            .sum::<u64>();
        let spill = full && self.full.is_some();
        self.merge_records(records, spill);
        self.reskip_protected();
        self.reskip_excluded();
        if full {
            let scanned_before = self.items_scanned;
            self.items_scanned += scanned_items;
            self.log_scan_progress(scanned_before);
        }
    }

    fn log_scan_progress(&self, scanned_before: u64) {
        let step_before = scanned_before / SCAN_PROGRESS_LOG_STEP;
        let step_now = self.items_scanned / SCAN_PROGRESS_LOG_STEP;
        if step_now != step_before {
            log::info!("size index: full scan reached {} items", self.items_scanned);
        }
    }

    fn merge_records(&self, records: Vec<DirRecord>, spill: bool) {
        let kept: Vec<DirRecord> = records
            .into_iter()
            .filter(|record| !self.shared.user_excludes(&record.path))
            .collect();
        let mut view = self.shared.view_mut();
        for record in kept {
            view.merge_record(record, spill);
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
    }

    pub(super) fn boost_done(&mut self, job: BoostJob, outcome: WalkOutcome) {
        let path = platform::normalize(Path::new(&job.path));
        self.walk_finished(&path, &outcome);
        if self.shared.user_excludes(&path) {
            answer_waiters(job.waiters, &unknown(&job.path));
            return;
        }
        let update = match &outcome {
            WalkOutcome::Failed(ManualSizeError::Cancelled) => return,
            WalkOutcome::Failed(_) => size_error(&job.path),
            WalkOutcome::Walked | WalkOutcome::Skipped => self.walked_update(&job.path, &path),
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

    pub(super) fn walk_finished(&mut self, root: &Path, outcome: &WalkOutcome) {
        match outcome {
            WalkOutcome::Walked => {
                self.shared.view_mut().prune_unscanned(root);
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

impl View {
    fn merge_record(&mut self, record: DirRecord, spill: bool) {
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
        let (Some(slot), Some(names)) = (slot, file_names) else {
            return;
        };
        if spill {
            self.names.spill_names(slot, names);
            return;
        }
        self.names.set_names(slot, names);
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

    fn prune_unscanned(&mut self, path: &Path) {
        self.tree.prune_unscanned(path);
        self.tombstone_removed();
    }

    fn clear_names(&mut self, slot: u32) {
        if self.keeps_file_names {
            self.names.set_names(slot, PackedNames::default());
        }
    }

    fn tombstone_removed(&mut self) {
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

fn log_rescan_reason(stale: Option<&Snapshot>, names_paired: bool) {
    let reason = rescan_reason(stale, names_paired);
    log::info!("size index: a full scan is needed, {reason}");
}

fn rescan_reason(stale: Option<&Snapshot>, names_paired: bool) -> &'static str {
    if stale.is_none() {
        return "no usable snapshot";
    }
    if !names_paired {
        return "the file-name file is missing or does not match the snapshot";
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
