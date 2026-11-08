use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crossbeam_channel::{bounded, unbounded, Sender};

use super::platform::{self, IndexPlatform};
use super::tree::{FolderSizeTree, FolderTotal};
use crate::resource_coordinator::ResourceCoordinator;
use crate::size::fast_walk::{self, Qos, WalkOptions};
use crate::size::{
    route_of, FolderSizeIndexStatus, IndexBackend, IndexState, NaReason, RequestMode, Route,
    SizeSource, SizeStateKind, SizeUpdate,
};

use super::lanes::{run_background, run_boosts, BoostJob, Lanes, ScanJob};
use super::machine::{Machine, Message};

pub type SizeEmitter = Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>;
pub type StatusEmitter = Arc<dyn Fn(FolderSizeIndexStatus) + Send + Sync>;
/// Wall-clock source; injected so tests control time.
pub type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

pub const SNAPSHOT_FILE: &str = "folder-size-index.bin";
/// Background work re-requests Throughput admission this often.
pub const READMIT_EVERY_DIRS: u64 = 1_000;
pub(super) const RECORD_BATCH: usize = 256;
pub(super) const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub(super) const FLUSH_TIMEOUT: Duration = Duration::from_secs(3);

/// Everything the index needs; `lib.rs` builds it with
/// [`IndexOptions::for_this_mac`], tests inject their own.
pub struct IndexOptions {
    pub roots: Vec<PathBuf>,
    /// Paths inside the roots that are never indexed, walked or watched.
    pub excluded: Vec<PathBuf>,
    pub protected: Vec<PathBuf>,
    pub stream_excluded: Vec<PathBuf>,
    pub snapshot_path: PathBuf,
    pub platform: Arc<dyn IndexPlatform>,
    pub clock: Clock,
    pub coordinator: Arc<ResourceCoordinator>,
    /// Live `size://state` updates for subscribed folders.
    pub emit_sizes: SizeEmitter,
    pub emit_status: StatusEmitter,
    #[cfg(feature = "test-utils")]
    pub walk_hooks: fast_walk::TestHooks,
    /// Runs each time a background walk parks for the power gate or a
    /// waiting boost, just before it waits.
    #[cfg(feature = "test-utils")]
    pub on_background_parked: Arc<dyn Fn() + Send + Sync>,
}

/// The walk lanes at one instant, for tests.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneSnapshot {
    pub background_queued: usize,
    pub background_running: bool,
    /// Queued boost paths, next to run first.
    pub boosts_queued: Vec<String>,
    pub boost_running: Option<String>,
    pub power_paused: bool,
}

#[cfg(not(feature = "test-utils"))]
impl IndexOptions {
    pub fn for_this_mac(
        cache_dir: &Path,
        coordinator: Arc<ResourceCoordinator>,
        emit_sizes: SizeEmitter,
        emit_status: StatusEmitter,
    ) -> Self {
        let home = platform::home_dir();
        Self {
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
            snapshot_path: cache_dir.join(SNAPSHOT_FILE),
            platform: Arc::new(platform::MacPlatform),
            clock: Arc::new(SystemTime::now),
            coordinator,
            emit_sizes,
            emit_status,
        }
    }
}

/// Why [`FolderSizeIndex::read_tree`] could not read the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeUnavailable {
    /// The snapshot has not loaded yet.
    NotReady,
    /// The change stream failed to start.
    Unavailable,
}

pub struct FolderSizeIndex {
    shared: Arc<Shared>,
    sender: Sender<Message>,
    threads: Vec<JoinHandle<()>>,
}

pub(super) struct Shared {
    pub(super) view: RwLock<View>,
    pub(super) status: Mutex<FolderSizeIndexStatus>,
    pub(super) lanes: Mutex<Lanes>,
    pub(super) wake: Condvar,
    pub(super) roots: Vec<PathBuf>,
    pub(super) excluded: Vec<PathBuf>,
    pub(super) protected: Vec<PathBuf>,
    pub(super) coordinator: Arc<ResourceCoordinator>,
    /// Set once the snapshot is loaded; earlier requests queue on the
    /// coordination thread instead of walking an empty tree.
    pub(super) booted: AtomicBool,
    #[cfg(feature = "test-utils")]
    pub(super) walk_hooks: fast_walk::TestHooks,
    #[cfg(feature = "test-utils")]
    pub(super) on_background_parked: Arc<dyn Fn() + Send + Sync>,
}

pub(super) struct View {
    pub(super) tree: FolderSizeTree,
    pub(super) visited: HashSet<PathBuf>,
    pub(super) full_disk_access: bool,
    /// False once the stream failed to start; everything routes manual.
    pub(super) available: bool,
}

impl FolderSizeIndex {
    pub fn start(options: IndexOptions) -> Self {
        let mut tree = FolderSizeTree::default();
        for root in &options.roots {
            tree.add_root(root);
        }
        let shared = Arc::new(Shared {
            view: RwLock::new(View {
                tree,
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
            coordinator: Arc::clone(&options.coordinator),
            booted: AtomicBool::new(false),
            #[cfg(feature = "test-utils")]
            walk_hooks: options.walk_hooks.clone(),
            #[cfg(feature = "test-utils")]
            on_background_parked: Arc::clone(&options.on_background_parked),
        });
        let (sender, receiver) = unbounded();
        let machine = Machine::new(Arc::clone(&shared), options, sender.clone());
        let threads = vec![
            spawn("size-index", move || machine.run(&receiver)),
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
            threads,
        }
    }

    /// Runs `read` under the shared read lock with the protected roots
    /// whose contents must stay hidden (the folder-size privacy rule).
    pub fn read_tree<R>(
        &self,
        read: impl FnOnce(&FolderSizeTree, &[PathBuf]) -> R,
    ) -> Result<R, TreeUnavailable> {
        if !self.shared.booted.load(Ordering::Acquire) {
            return Err(TreeUnavailable::NotReady);
        }
        let view = self.shared.view();
        if !view.available {
            return Err(TreeUnavailable::Unavailable);
        }
        let blocked = self.shared.privacy_blocked_paths(&view);
        Ok(read(&view.tree, &blocked))
    }

    /// Whether `path` lies inside an index root and outside the exclusions.
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

    /// Answers indexed paths at once; the rest are boosted and
    /// answered through `emitter` when their walk completes.
    pub fn request(&self, paths: Vec<String>, mode: RequestMode, emitter: SizeEmitter) {
        if !self.shared.booted.load(Ordering::Acquire) {
            let _ = self.sender.send(Message::Request {
                paths,
                mode,
                emitter,
            });
            return;
        }
        self.shared.request(paths, mode, emitter);
    }

    /// Drops queued boosts for `paths` and stops a running one; returns
    /// how many were cancelled.
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

    /// Replaces `pane`'s subscription set; an empty set with no current
    /// path clears it. A current path inside an unvisited protected root
    /// marks it visited before any later request is answered. Indexed
    /// paths through a symlink track their resolved target.
    pub fn set_subscriptions(
        &self,
        pane: String,
        current_path: Option<String>,
        folders: Vec<String>,
    ) {
        if let Some(root) =
            current_path.and_then(|path| self.shared.visit(&self.tracked(Path::new(&path))))
        {
            let _ = self.sender.send(Message::Visited(root));
        }
        let folders = folders
            .into_iter()
            .map(|folder| {
                let tracked = self.tracked(Path::new(&folder));
                (folder, tracked)
            })
            .collect();
        let _ = self.sender.send(Message::Subscribe { pane, folders });
    }

    /// The normalized path `path`'s total lives at. Only indexed paths
    /// are resolved, so other volumes and shares are never touched.
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

    /// Writes the snapshot on the index thread and waits (bounded) for it.
    pub fn flush(&self) {
        let (ack, done) = bounded(1);
        if self.sender.send(Message::Flush(ack)).is_ok() {
            let _ = done.recv_timeout(FLUSH_TIMEOUT);
        }
    }
}

impl Drop for FolderSizeIndex {
    fn drop(&mut self) {
        self.shared.shut_down_lanes();
        self.shared.wake.notify_all();
        let _ = self.sender.send(Message::Shutdown);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

pub(super) fn spawn(name: &str, body: impl FnOnce() + Send + 'static) -> JoinHandle<()> {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .expect("spawn size index thread")
}

impl Shared {
    fn shut_down_lanes(&self) {
        let mut lanes = self.lanes();
        lanes.shutdown = true;
        let running = lanes
            .running_background
            .iter()
            .chain(lanes.running_boost.iter().map(|(_, cancel)| cancel));
        for cancel in running {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn request(&self, paths: Vec<String>, mode: RequestMode, emitter: SizeEmitter) {
        let mut triage = RequestTriage::default();
        let view = self.view();
        for path in paths {
            let answer = self.answer(&view, &path, mode);
            triage.record(path, answer);
        }
        drop(view);
        if !triage.answered.is_empty() {
            emitter(triage.answered);
        }
        if !triage.boosted.is_empty() {
            self.queue_boosts(triage.boosted, mode == RequestMode::Refresh, Some(emitter));
        }
    }

    pub(super) fn view(&self) -> std::sync::RwLockReadGuard<'_, View> {
        self.view.read().expect("index view lock")
    }

    pub(super) fn view_mut(&self) -> std::sync::RwLockWriteGuard<'_, View> {
        self.view.write().expect("index view lock")
    }

    pub(super) fn lanes(&self) -> MutexGuard<'_, Lanes> {
        self.lanes.lock().expect("index lanes lock")
    }

    /// Inside a root and outside every exclusion (`path` normalized).
    pub(super) fn indexes(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| within(path, root))
            && !self.excluded.iter().any(|excluded| within(path, excluded))
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

    /// The indexed total of a subscribed folder, once it is complete and
    /// readable.
    pub(super) fn settled_update(
        &self,
        view: &View,
        path: &str,
        normalized: &Path,
    ) -> Option<SizeUpdate> {
        if self.privacy_blocked(view, normalized) {
            return None;
        }
        let total = view.tree.total(normalized)?;
        (total.fully_indexed && !total.skipped).then(|| update_from_total(path, &total))
    }

    /// Background and automatic work must not read `path`.
    pub(super) fn privacy_blocked(&self, view: &View, path: &Path) -> bool {
        !view.full_disk_access
            && self
                .protected_root_of(path)
                .is_some_and(|root| !view.visited.contains(root))
    }

    /// Marks the protected root containing `path` visited; returns it if
    /// it was not visited before.
    pub(super) fn visit(&self, path: &Path) -> Option<PathBuf> {
        let root = self.protected_root_of(&platform::normalize(path))?.clone();
        self.view_mut().visited.insert(root.clone()).then_some(root)
    }

    /// `None` means the path needs a boosted walk.
    pub(super) fn answer(&self, view: &View, path: &str, mode: RequestMode) -> Option<SizeUpdate> {
        let normalized = platform::normalize(Path::new(path));
        if self.privacy_blocked(view, &normalized) {
            return Some(unknown(path));
        }
        let total = view.tree.total(&normalized)?;
        (mode != RequestMode::Refresh && !total.skipped && total.fully_indexed)
            .then(|| update_from_total(path, &total))
    }

    pub(super) fn queue_boosts(
        &self,
        paths: Vec<String>,
        refresh: bool,
        waiter: Option<SizeEmitter>,
    ) {
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

    /// Queues a gentle background walk; false if already queued.
    pub(super) fn queue_background(&self, root: PathBuf, full: bool) -> bool {
        let mut lanes = self.lanes();
        if lanes.background.iter().any(|job| job.root == root) {
            return false;
        }
        lanes.background.push_back(ScanJob { root, full });
        self.wake.notify_all();
        true
    }

    pub(super) fn walk_options(
        &self,
        cancel: Arc<AtomicBool>,
        qos: Qos,
        threads: usize,
    ) -> WalkOptions {
        let mut options = WalkOptions::new(cancel, None).with_index_boundaries();
        options.excluded.extend(self.excluded.iter().cloned());
        options.qos = qos;
        options.threads = threads;
        #[cfg(feature = "test-utils")]
        {
            options.hooks = self.walk_hooks.clone();
        }
        options
    }
}

/// Exact prefix first, then a case-insensitive one.
pub(super) fn within(path: &Path, root: &Path) -> bool {
    path.starts_with(root) || lowercase(path).starts_with(lowercase(root))
}

pub(super) fn lowercase(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

pub(super) fn unknown(path: &str) -> SizeUpdate {
    SizeUpdate::new(
        path.to_string(),
        SizeStateKind::Unknown,
        SizeSource::Index,
        None,
    )
}

pub(super) fn update_from_total(path: &str, total: &FolderTotal) -> SizeUpdate {
    if total.denied {
        return SizeUpdate::na(path.to_string(), SizeSource::Index, Some(NaReason::Denied));
    }
    let mut update = SizeUpdate::new(
        path.to_string(),
        SizeStateKind::Ready,
        SizeSource::Index,
        Some(total.bytes),
    );
    if let Some(bound) = &total.lower_bound {
        update.is_lower_bound = true;
        update.skipped_roots =
            (!bound.skipped_names.is_empty()).then(|| bound.skipped_names.clone());
        update.skipped_more = (bound.skipped_more > 0).then_some(bound.skipped_more);
    }
    update
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
    SizeUpdate::new(
        path.to_string(),
        SizeStateKind::Calculating,
        SizeSource::Index,
        None,
    )
}
