use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crossbeam_channel::{bounded, unbounded, Sender};

use super::exclusions::{ExclusionChange, ExclusionRules, ExclusionSet};
use super::names::file::NAMES_FILE;
use super::names::FileNameStore;
use super::platform::{self, IndexPlatform};
use super::tree::{FolderSizeTree, FolderTotal, LowerBound};
use super::IndexRead;
use crate::resource_coordinator::ResourceCoordinator;
use crate::size::fast_walk::{self, Qos, WalkOptions};
use crate::size::{
    route_of, FolderSizeIndexStatus, FolderSizeIndexStorage, IndexBackend, IndexState, NaReason,
    PaneSubscriptions, RequestMode, Route, SizeRequest, SizeSource, SizeStateKind, SizeUpdate,
};

use super::compact::run_compactions;
use super::lanes::{run_background, run_boosts, BoostJob, Lanes, ScanJob};
use super::machine::{ExclusionEdit, Machine, MachineLinks, Message};
use super::segment::sequence_of_segment_file;
use super::subscriptions::SubscribedFolder;

pub type SizeEmitter = Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>;
pub type StatusEmitter = Arc<dyn Fn(FolderSizeIndexStatus) + Send + Sync>;
pub type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

pub const SNAPSHOT_FILE: &str = "folder-size-index.bin";
pub const READMIT_EVERY_DIRS: u64 = 1_000;
pub const HEAP_BATCH_RECORDS: usize = 4_096;
pub(super) const RECORD_BATCH: usize = 256;
pub(super) const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub(super) const FLUSH_TIMEOUT: Duration = Duration::from_secs(3);

const FIRST_SEQUENCE: u64 = 1;

pub(super) struct SegmentSequences {
    next: u64,
    building: Vec<u64>,
}

impl Default for SegmentSequences {
    fn default() -> Self {
        Self {
            next: FIRST_SEQUENCE,
            building: Vec::new(),
        }
    }
}

impl SegmentSequences {
    pub(super) fn claim(&mut self) -> u64 {
        let sequence = self.next;
        self.next += 1;
        self.building.push(sequence);
        sequence
    }

    pub(super) fn release(&mut self, sequence: u64) {
        self.building.retain(|&building| building != sequence);
    }

    pub(super) fn is_building(&self, sequence: u64) -> bool {
        self.building.contains(&sequence)
    }

    pub(super) fn skip_past(&mut self, used: u64) {
        self.next = self.next.max(used + 1);
    }
}

#[derive(Clone)]
pub struct IndexOptions {
    pub collect_file_names: bool,
    pub roots: Vec<PathBuf>,
    pub excluded: Vec<PathBuf>,
    pub protected: Vec<PathBuf>,
    pub stream_excluded: Vec<PathBuf>,
    pub user_exclusions: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub snapshot_path: PathBuf,
    pub heap_batch_records: usize,
    pub platform: Arc<dyn IndexPlatform>,
    pub clock: Clock,
    pub coordinator: Arc<ResourceCoordinator>,
    pub emit_sizes: SizeEmitter,
    pub emit_status: StatusEmitter,
    #[cfg(feature = "test-utils")]
    pub walk_hooks: fast_walk::TestHooks,
    #[cfg(feature = "test-utils")]
    pub on_background_parked: Arc<dyn Fn() + Send + Sync>,
}

#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneSnapshot {
    pub background_queued: usize,
    pub background_running: bool,
    pub boosts_queued: Vec<String>,
    pub boost_running: Option<String>,
    pub power_paused: bool,
}

#[cfg(not(feature = "test-utils"))]
pub struct MacIndexSetup<'setup> {
    pub cache_dir: &'setup Path,
    pub user_exclusions: Vec<PathBuf>,
    pub emit_sizes: SizeEmitter,
    pub emit_status: StatusEmitter,
}

#[cfg(not(feature = "test-utils"))]
impl IndexOptions {
    pub fn for_this_mac(setup: MacIndexSetup<'_>, coordinator: Arc<ResourceCoordinator>) -> Self {
        let MacIndexSetup {
            cache_dir,
            user_exclusions,
            emit_sizes,
            emit_status,
        } = setup;
        let home = platform::home_dir();
        Self {
            collect_file_names: false,
            roots: vec![PathBuf::from("/")],
            excluded: fast_walk::INDEX_EXCLUDED_ROOTS
                .iter()
                .map(PathBuf::from)
                .collect(),
            protected: home
                .as_deref()
                .map(platform::protected_roots)
                .unwrap_or_default(),
            stream_excluded: home
                .as_deref()
                .map(platform::stream_exclusions)
                .unwrap_or_default(),
            user_exclusions,
            home,
            snapshot_path: cache_dir.join(SNAPSHOT_FILE),
            heap_batch_records: HEAP_BATCH_RECORDS,
            platform: Arc::new(platform::MacPlatform),
            clock: Arc::new(SystemTime::now),
            coordinator,
            emit_sizes,
            emit_status,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeUnavailable {
    NotReady,
    Unavailable,
}

fn segment_directory_of(snapshot_path: &Path) -> PathBuf {
    snapshot_path
        .parent()
        .unwrap_or(snapshot_path)
        .to_path_buf()
}

pub fn names_path_beside(snapshot_path: &Path) -> PathBuf {
    snapshot_path.with_file_name(NAMES_FILE)
}

pub struct FolderSizeIndex {
    shared: Arc<Shared>,
    sender: Sender<Message>,
    threads: Mutex<Vec<JoinHandle<()>>>,
    snapshot_path: PathBuf,
}

struct IndexedItems {
    folders: u64,
    files: Option<u64>,
}

impl IndexedItems {
    fn counted(read: IndexRead<'_>) -> Self {
        Self {
            folders: read.tree.node_count() as u64,
            files: counted_file_names(read.names),
        }
    }
}

fn counted_file_names(names: &FileNameStore) -> Option<u64> {
    if !names.is_available() {
        return None;
    }
    names.count_saved_and_overlay_names().ok()
}

fn is_index_file(path: &Path, snapshot_path: &Path) -> bool {
    let is_segment_file = path
        .file_name()
        .is_some_and(|name| sequence_of_segment_file(&name.to_string_lossy()).is_some());
    is_segment_file || path == snapshot_path
}

fn index_bytes_on_disk(snapshot_path: &Path) -> u64 {
    let directory = snapshot_path.parent().unwrap_or(snapshot_path);
    let Ok(entries) = fs::read_dir(directory) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| is_index_file(&entry.path(), snapshot_path))
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

pub(super) struct Shared {
    pub(super) view: RwLock<View>,
    pub(super) status: Mutex<FolderSizeIndexStatus>,
    pub(super) lanes: Mutex<Lanes>,
    pub(super) wake: Condvar,
    pub(super) roots: Vec<PathBuf>,
    pub(super) excluded: Vec<PathBuf>,
    pub(super) protected: Vec<PathBuf>,
    pub(super) exclusions: RwLock<ExclusionSet>,
    pub(super) exclusion_rules: ExclusionRules,
    pub(super) coordinator: Arc<ResourceCoordinator>,
    pub(super) collect_file_names: bool,
    pub(super) heap_batch_records: usize,
    pub(super) segment_directory: PathBuf,
    pub(super) sequences: Mutex<SegmentSequences>,
    pub(super) booted: AtomicBool,
    #[cfg(feature = "test-utils")]
    pub(super) walk_hooks: fast_walk::TestHooks,
    #[cfg(feature = "test-utils")]
    pub(super) on_background_parked: Arc<dyn Fn() + Send + Sync>,
}

pub(super) struct View {
    pub(super) tree: FolderSizeTree,
    pub(super) generation: u64,
    pub(super) names: FileNameStore,
    pub(super) keeps_file_names: bool,
    pub(super) visited: HashSet<PathBuf>,
    pub(super) full_disk_access: bool,
    pub(super) available: bool,
}

impl FolderSizeIndex {
    pub fn start(options: IndexOptions) -> Self {
        let shared = Arc::new(Shared::starting_with(&options));
        let snapshot_path = options.snapshot_path.clone();
        let (sender, receiver) = unbounded();
        let (compact_jobs, compact_queue) = unbounded();
        let links = MachineLinks {
            sender: sender.clone(),
            compact_jobs,
        };
        let machine = Machine::new(Arc::clone(&shared), options, links);
        let threads = vec![
            spawn("size-index", move || machine.run(&receiver)),
            spawn("size-index-compact", {
                let (shared, sender) = (Arc::clone(&shared), sender.clone());
                move || run_compactions(&shared, &compact_queue, &sender)
            }),
            spawn("size-index-scan", {
                let (shared, sender) = (Arc::clone(&shared), sender.clone());
                move || run_background(&shared, &sender)
            }),
            spawn("size-index-boost", {
                let (shared, sender) = (Arc::clone(&shared), sender.clone());
                move || run_boosts(&shared, &sender)
            }),
        ];
        Self {
            shared,
            sender,
            threads: Mutex::new(threads),
            snapshot_path,
        }
    }

    pub fn read_index<R>(
        &self,
        read: impl FnOnce(IndexRead<'_>) -> R,
    ) -> Result<R, TreeUnavailable> {
        if !self.shared.is_booted() {
            return Err(TreeUnavailable::NotReady);
        }
        let view = self.shared.view();
        if !view.available {
            return Err(TreeUnavailable::Unavailable);
        }
        let blocked = self.shared.privacy_blocked_paths(&view);
        let result = read(IndexRead {
            tree: &view.tree,
            names: &view.names,
            blocked: &blocked,
            generation: view.generation,
        });
        let names_failed = view.names.take_unreported_failure();
        let tree_failed = view.tree.is_corrupt();
        drop(view);
        if names_failed {
            let _ = self.sender.send(Message::NamesUnreadable);
        }
        if tree_failed {
            let _ = self.sender.send(Message::TreeUnreadable);
        }
        Ok(result)
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.shared.view().available && self.shared.indexes(&platform::normalize(path))
    }

    pub fn status(&self) -> FolderSizeIndexStatus {
        self.shared
            .status
            .lock()
            .expect("index status lock")
            .clone()
    }

    pub fn request(&self, request: SizeRequest, emitter: SizeEmitter) {
        if !self.shared.is_booted() {
            let _ = self.sender.send(Message::Request { request, emitter });
            return;
        }
        self.shared.request(request, emitter);
    }

    pub fn cancel(&self, paths: &[String]) -> usize {
        let mut lanes = self.shared.lanes();
        let before = lanes.boosts.len();
        lanes.boosts.retain(|job| !paths.contains(&job.path));
        let dropped_queued = before - lanes.boosts.len();
        let running_cancel = lanes
            .running_boost
            .as_ref()
            .filter(|(path, _)| paths.contains(path))
            .map(|(_, cancel)| cancel);
        if let Some(cancel) = running_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        let cancelled = dropped_queued + usize::from(running_cancel.is_some());
        self.shared.wake.notify_all();
        cancelled
    }

    pub fn set_subscriptions(&self, subscriptions: PaneSubscriptions) {
        let PaneSubscriptions {
            pane_id: pane,
            current_path,
            folders,
        } = subscriptions;
        if let Some(root) =
            current_path.and_then(|path| self.shared.visit(&self.tracked(Path::new(&path))))
        {
            let _ = self.sender.send(Message::Visited(root));
        }
        let folders = folders
            .into_iter()
            .map(|path| {
                let normalized = self.tracked(Path::new(&path));
                SubscribedFolder { path, normalized }
            })
            .collect();
        let _ = self.sender.send(Message::Subscribe { pane, folders });
    }

    pub(super) fn tracked(&self, path: &Path) -> PathBuf {
        if !self.contains(path) {
            return platform::normalize(path);
        }
        match route_of(path) {
            Route::Target(target) => platform::normalize(&target),
            Route::AsIs | Route::NotDir => platform::normalize(path),
        }
    }

    pub fn focus_changed(&self, focused: bool) {
        let _ = self.sender.send(Message::Focus(focused));
    }

    #[cfg(feature = "test-utils")]
    pub fn lanes_for_tests(&self) -> LaneSnapshot {
        let lanes = self.shared.lanes();
        LaneSnapshot {
            background_queued: lanes.background.len(),
            background_running: lanes.running_background.is_some(),
            boosts_queued: lanes.boosts.iter().map(|job| job.path.clone()).collect(),
            boost_running: lanes.running_boost.as_ref().map(|(path, _)| path.clone()),
            power_paused: lanes.power_paused,
        }
    }

    pub fn exclusions(&self) -> Vec<PathBuf> {
        self.shared.exclusions().paths().to_vec()
    }

    pub fn add_exclusions(&self, candidates: &[PathBuf]) -> ExclusionChange {
        let mut set = self.shared.exclusions_mut();
        let verdicts = candidates
            .iter()
            .map(|candidate| set.add(candidate, &self.shared.exclusion_rules))
            .collect();
        let exclusions = set.paths().to_vec();
        drop(set);
        let change = ExclusionChange {
            exclusions,
            verdicts,
        };
        let added = change.added_paths();
        if !added.is_empty() {
            self.apply_exclusion_edit(ExclusionEdit {
                added,
                removed: Vec::new(),
            });
        }
        change
    }

    pub fn remove_exclusion(&self, path: &Path) -> Vec<PathBuf> {
        let mut set = self.shared.exclusions_mut();
        let removed = set.remove(path);
        let exclusions = set.paths().to_vec();
        drop(set);
        if let Some(removed) = removed {
            self.apply_exclusion_edit(ExclusionEdit {
                added: Vec::new(),
                removed: vec![removed],
            });
        }
        exclusions
    }

    fn apply_exclusion_edit(&self, edit: ExclusionEdit) {
        self.send_and_wait(|ack| Message::Exclusions { edit, ack });
    }

    pub fn retry_stream(&self) {
        self.send_and_wait(Message::RetryStream);
    }

    pub fn rebuild(&self) {
        self.send_and_wait(Message::Rebuild);
    }

    pub fn flush(&self) {
        self.send_and_wait(Message::Flush);
    }

    fn send_and_wait(&self, message: impl FnOnce(Sender<()>) -> Message) {
        let (ack, done) = bounded(1);
        if self.sender.send(message(ack)).is_ok() {
            let _ = done.recv_timeout(FLUSH_TIMEOUT);
        }
    }

    pub fn storage(&self) -> FolderSizeIndexStorage {
        let items = self.read_index(IndexedItems::counted).ok();
        let directory = self.snapshot_path.parent().unwrap_or(&self.snapshot_path);
        FolderSizeIndexStorage {
            directory: directory.to_string_lossy().into_owned(),
            size_bytes: index_bytes_on_disk(&self.snapshot_path),
            folder_count: items.as_ref().map(|items| items.folders),
            file_count: items.and_then(|items| items.files),
        }
    }
}

impl FolderSizeIndex {
    pub fn stop(&self) {
        self.flush();
        self.shut_down();
    }

    fn shut_down(&self) {
        self.shared.shut_down_lanes();
        self.shared.wake.notify_all();
        let _ = self.sender.send(Message::Shutdown);
        let mut threads = self.threads.lock().expect("index threads lock");
        for thread in threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for FolderSizeIndex {
    fn drop(&mut self) {
        self.shut_down();
    }
}

pub(super) fn spawn(name: &str, body: impl FnOnce() + Send + 'static) -> JoinHandle<()> {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .expect("spawn size index thread")
}

impl Shared {
    fn starting_with(options: &IndexOptions) -> Self {
        let mut tree = FolderSizeTree::default();
        for root in &options.roots {
            tree.add_root(root);
        }
        Self {
            view: RwLock::new(View {
                tree,
                generation: 0,
                names: FileNameStore::new(names_path_beside(&options.snapshot_path)),
                keeps_file_names: options.collect_file_names,
                visited: HashSet::new(),
                full_disk_access: false,
                available: true,
            }),
            status: Mutex::new(FolderSizeIndexStatus::new(
                IndexBackend::MacIndex,
                true,
                IndexState::NotReady,
            )),
            lanes: Mutex::new(Lanes::default()),
            wake: Condvar::new(),
            roots: options.roots.clone(),
            excluded: options.excluded.clone(),
            protected: options.protected.clone(),
            exclusions: RwLock::new(ExclusionSet::new(options.user_exclusions.clone())),
            exclusion_rules: ExclusionRules {
                roots: options.roots.clone(),
                always_skipped: options.excluded.clone(),
                alias_prefix: Some(PathBuf::from(platform::DATA_VOLUME_PREFIX)),
                home: options.home.clone(),
            },
            coordinator: Arc::clone(&options.coordinator),
            collect_file_names: options.collect_file_names,
            heap_batch_records: options.heap_batch_records,
            segment_directory: segment_directory_of(&options.snapshot_path),
            sequences: Mutex::new(SegmentSequences::default()),
            booted: AtomicBool::new(false),
            #[cfg(feature = "test-utils")]
            walk_hooks: options.walk_hooks.clone(),
            #[cfg(feature = "test-utils")]
            on_background_parked: Arc::clone(&options.on_background_parked),
        }
    }

    fn is_booted(&self) -> bool {
        self.booted.load(Ordering::Acquire)
    }

    fn shut_down_lanes(&self) {
        let mut lanes = self.lanes();
        lanes.shutdown = true;
        let running = lanes
            .running_background
            .iter()
            .map(|running| &running.cancel)
            .chain(lanes.running_boost.iter().map(|(_, cancel)| cancel));
        for cancel in running {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn request(&self, request: SizeRequest, emitter: SizeEmitter) {
        let SizeRequest { paths, mode } = request;
        let mut triage = RequestTriage::default();
        let view = self.view();
        let query = ViewQuery { view: &view, mode };
        for path in paths {
            let answer = self.answer(query, &path);
            triage.record(path, answer);
        }
        drop(view);
        let RequestTriage { answered, boosted } = triage;
        if !answered.is_empty() {
            emitter(answered);
        }
        if !boosted.is_empty() {
            self.queue_boosts(BoostRequest {
                paths: boosted,
                refresh: mode == RequestMode::Refresh,
                waiter: Some(emitter),
            });
        }
    }

    pub(super) fn view(&self) -> std::sync::RwLockReadGuard<'_, View> {
        self.view.read().expect("index view lock")
    }

    pub(super) fn view_mut(&self) -> std::sync::RwLockWriteGuard<'_, View> {
        self.view.write().expect("index view lock")
    }

    pub(super) fn sequences(&self) -> MutexGuard<'_, SegmentSequences> {
        self.sequences.lock().expect("index sequences lock")
    }

    pub(super) fn lanes(&self) -> MutexGuard<'_, Lanes> {
        self.lanes.lock().expect("index lanes lock")
    }

    pub(super) fn exclusions(&self) -> std::sync::RwLockReadGuard<'_, ExclusionSet> {
        self.exclusions.read().expect("index exclusions lock")
    }

    fn exclusions_mut(&self) -> std::sync::RwLockWriteGuard<'_, ExclusionSet> {
        self.exclusions.write().expect("index exclusions lock")
    }

    pub(super) fn user_excludes(&self, path: &Path) -> bool {
        self.exclusions().covers(path)
    }

    pub(super) fn indexes(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| within(path, root))
            && !self.excluded.iter().any(|excluded| within(path, excluded))
            && !self.user_excludes(path)
    }

    pub(super) fn protected_root_of(&self, path: &Path) -> Option<&PathBuf> {
        self.protected.iter().find(|root| within(path, root))
    }

    fn privacy_blocked_paths(&self, view: &View) -> Vec<PathBuf> {
        if view.full_disk_access {
            return Vec::new();
        }
        self.unvisited_protected(view)
    }

    pub(super) fn unvisited_protected(&self, view: &View) -> Vec<PathBuf> {
        self.protected
            .iter()
            .filter(|root| !view.visited.contains(*root))
            .cloned()
            .collect()
    }

    pub(super) fn settled_update(
        &self,
        view: &View,
        folder: &SubscribedFolder,
    ) -> Option<SizeUpdate> {
        if self.privacy_blocked(view, &folder.normalized) {
            return None;
        }
        let total = view.tree.total(&folder.normalized)?;
        (total.fully_indexed && !total.skipped)
            .then(|| self.update_from_total(&folder.path, &total))
    }

    pub(super) fn privacy_blocked(&self, view: &View, path: &Path) -> bool {
        !view.full_disk_access
            && self
                .protected_root_of(path)
                .is_some_and(|root| !view.visited.contains(root))
    }

    pub(super) fn visit(&self, path: &Path) -> Option<PathBuf> {
        let root = self.protected_root_of(&platform::normalize(path))?.clone();
        self.view_mut().visited.insert(root.clone()).then_some(root)
    }

    pub(super) fn answer(&self, query: ViewQuery<'_>, path: &str) -> Option<SizeUpdate> {
        let ViewQuery { view, mode } = query;
        let normalized = platform::normalize(Path::new(path));
        if self.privacy_blocked(view, &normalized) {
            return Some(unknown(path));
        }
        let total = view.tree.total(&normalized)?;
        (mode != RequestMode::Refresh && !total.skipped && total.fully_indexed)
            .then(|| self.update_from_total(path, &total))
    }

    pub(super) fn queue_boosts(&self, request: BoostRequest) {
        let BoostRequest {
            paths,
            refresh,
            waiter,
        } = request;
        let mut lanes = self.lanes();
        for path in paths {
            let mut job = match lanes.boosts.iter().position(|job| job.path == path) {
                Some(at) => lanes.boosts.remove(at).expect("queued boost"),
                None => BoostJob {
                    path,
                    refresh,
                    waiters: Vec::new(),
                },
            };
            job.refresh |= refresh;
            job.waiters.extend(waiter.clone());
            lanes.boosts.push_front(job);
        }
        self.wake.notify_all();
    }

    pub(super) fn queue_background(&self, job: ScanJob) -> bool {
        let mut lanes = self.lanes();
        if lanes
            .background
            .iter()
            .any(|queued| queued.root == job.root)
        {
            return false;
        }
        lanes.background.push_back(job);
        self.wake.notify_all();
        true
    }

    pub(super) fn walk_options(&self, cancel: Arc<AtomicBool>, tuning: WalkTuning) -> WalkOptions {
        let mut options = WalkOptions::new(cancel, None).with_index_boundaries();
        options.excluded.extend(self.excluded.iter().cloned());
        options
            .excluded
            .extend(self.exclusions().paths().iter().cloned());
        options.collect_file_names = self.collect_file_names;
        options.qos = tuning.qos;
        options.threads = tuning.threads;
        #[cfg(feature = "test-utils")]
        {
            options.hooks = self.walk_hooks.clone();
        }
        options
    }

    pub(super) fn update_from_total(&self, path: &str, total: &FolderTotal) -> SizeUpdate {
        if total.denied {
            return SizeUpdate::na(path.to_string(), SizeSource::Index, Some(NaReason::Denied));
        }
        let mut update = SizeUpdate::ready(path.to_string(), SizeSource::Index, total.bytes);
        if let Some(bound) = &total.lower_bound {
            let named = self.skipped_names(bound);
            update.is_lower_bound = true;
            update.skipped_roots = non_empty(named.privacy);
            update.excluded_roots = non_empty(named.excluded);
            update.skipped_more = (bound.skipped_more > 0).then_some(bound.skipped_more);
        }
        update
    }

    fn skipped_names(&self, bound: &LowerBound) -> SkippedNames {
        let exclusions = self.exclusions();
        let mut names = SkippedNames::default();
        for (name, path) in bound.skipped_names.iter().zip(&bound.skipped_paths) {
            names.record(name, exclusions.covers(path));
        }
        names
    }
}

#[derive(Default)]
struct SkippedNames {
    privacy: Vec<String>,
    excluded: Vec<String>,
}

impl SkippedNames {
    fn record(&mut self, name: &str, user_excluded: bool) {
        if user_excluded {
            self.excluded.push(name.to_string());
            return;
        }
        self.privacy.push(name.to_string());
    }
}

fn non_empty(names: Vec<String>) -> Option<Vec<String>> {
    (!names.is_empty()).then_some(names)
}

#[derive(Clone, Copy)]
pub(super) struct ViewQuery<'view> {
    pub(super) view: &'view View,
    pub(super) mode: RequestMode,
}

pub(super) struct BoostRequest {
    pub(super) paths: Vec<String>,
    pub(super) refresh: bool,
    pub(super) waiter: Option<SizeEmitter>,
}

#[derive(Clone, Copy)]
pub(super) struct WalkTuning {
    pub(super) qos: Qos,
    pub(super) threads: usize,
}

pub(super) fn within(path: &Path, root: &Path) -> bool {
    path.starts_with(root) || lowercase(path).starts_with(lowercase(root))
}

pub(super) fn lowercase(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

pub(super) fn unknown(path: &str) -> SizeUpdate {
    SizeUpdate::without_size(path.to_string(), SizeStateKind::Unknown, SizeSource::Index)
}

pub(super) fn unix_secs(time: SystemTime) -> u64 {
    crate::epoch::since_epoch(time).as_secs()
}

#[derive(Default)]
struct RequestTriage {
    answered: Vec<SizeUpdate>,
    boosted: Vec<String>,
}

impl RequestTriage {
    fn record(&mut self, path: String, answer: Option<SizeUpdate>) {
        if let Some(update) = answer {
            self.answered.push(update);
            return;
        }
        self.answered.push(calculating_update(&path));
        self.boosted.push(path);
    }
}

fn calculating_update(path: &str) -> SizeUpdate {
    SizeUpdate::without_size(
        path.to_string(),
        SizeStateKind::Calculating,
        SizeSource::Index,
    )
}
