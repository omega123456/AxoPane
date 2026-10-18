//! Folder-size index. The pure tree and persistence modules are
//! cross-platform; the macOS service builds on them.
//!
//! The service keeps one folder-only tree of `/`, minus other volumes,
//! `/System/Volumes` and devfs
//! ([`crate::size::fast_walk::INDEX_EXCLUDED_ROOTS`]), never crossing onto a
//! device other than the root's or the Data volume's. Symlinks are never
//! stored or followed; a size request through one is routed by its resolved
//! target. A coordination thread applies every tree mutation: scan records, single-directory re-reads driven by a recursive,
//! directory-level FSEvents stream, and privacy changes. Lookups take a
//! shared read lock on the caller's thread. Two worker threads walk with
//! [`crate::size::fast_walk`]: a gentle background lane (one thread,
//! background QoS, re-admitted every ~1,000 directories, paused by the power
//! gate and while a boost waits) and a boost lane that runs one user-visible
//! walk at a time, newest first.
//!
//! The stream is used only for size bookkeeping: it never emits `dir://patch`
//! and never drives pane or tree refresh.

pub mod persist;
#[cfg(target_os = "macos")]
pub mod platform;
pub mod tree;

#[cfg(target_os = "macos")]
pub use service::*;

#[cfg(target_os = "macos")]
mod service {
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

    use super::persist::{self, ChangeEvent, ReplayOutcome, SnapshotMeta};
    use super::platform::{self, IndexPlatform, PowerSignal, RawEvent, StreamHandle};
    use super::tree::{FolderSizeTree, FolderTotal};
    use crate::resource_coordinator::{JobClass, JobHandle, JobSpec, ResourceCoordinator};
    use crate::size::fast_walk::{self, DirRecord, Qos, WalkOptions, MAX_JOB_THREADS};
    use crate::size::manual::ManualSizeError;
    use crate::size::{
        route_of, FolderSizeIndexStatus, IndexBackend, IndexState, NaReason, PauseReason,
        RequestMode, Route, SizeSource, SizeStateKind, SizeUpdate,
    };

    pub type SizeEmitter = Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>;
    pub type StatusEmitter = Arc<dyn Fn(FolderSizeIndexStatus) + Send + Sync>;
    /// Wall-clock source; injected so tests control time.
    pub type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

    pub const SNAPSHOT_FILE: &str = "folder-size-index.bin";
    /// Background work re-requests Throughput admission this often.
    pub const READMIT_EVERY_DIRS: u64 = 1_000;
    const RECORD_BATCH: usize = 256;
    const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
    const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
    const FLUSH_TIMEOUT: Duration = Duration::from_secs(3);

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

    pub struct FolderSizeIndex {
        shared: Arc<Shared>,
        tx: Sender<Msg>,
        threads: Vec<JoinHandle<()>>,
    }

    struct Shared {
        view: RwLock<View>,
        status: Mutex<FolderSizeIndexStatus>,
        lanes: Mutex<Lanes>,
        wake: Condvar,
        roots: Vec<PathBuf>,
        excluded: Vec<PathBuf>,
        protected: Vec<PathBuf>,
        coordinator: Arc<ResourceCoordinator>,
        /// Set once the snapshot is loaded; earlier requests queue on the
        /// coordination thread instead of walking an empty tree.
        booted: AtomicBool,
        #[cfg(feature = "test-utils")]
        walk_hooks: fast_walk::TestHooks,
        #[cfg(feature = "test-utils")]
        on_background_parked: Arc<dyn Fn() + Send + Sync>,
    }

    struct View {
        tree: FolderSizeTree,
        visited: HashSet<PathBuf>,
        full_disk_access: bool,
        /// False once the stream failed to start; everything routes manual.
        available: bool,
    }

    #[derive(Default)]
    struct Lanes {
        background: VecDeque<ScanJob>,
        boosts: VecDeque<BoostJob>,
        running_background: Option<Arc<AtomicBool>>,
        running_boost: Option<(String, Arc<AtomicBool>)>,
        /// Sleep, Low Power Mode or thermal pressure (FR4.1).
        power_paused: bool,
        shutdown: bool,
    }

    impl Lanes {
        fn background_must_wait(&self) -> bool {
            self.power_paused || !self.boosts.is_empty() || self.running_boost.is_some()
        }
    }

    struct ScanJob {
        root: PathBuf,
        /// Part of a cold or full rescan (counts toward progress).
        full: bool,
    }

    struct BoostJob {
        path: String,
        refresh: bool,
        waiters: Vec<SizeEmitter>,
    }

    enum WalkOutcome {
        Walked,
        /// Nothing to walk: privacy-blocked or already indexed.
        Skipped,
        Failed(ManualSizeError),
    }

    enum Msg {
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
            paths: Vec<String>,
            mode: RequestMode,
            emitter: SizeEmitter,
        },
        Subscribe {
            pane: String,
            /// Display paths with the normalized paths they track.
            folders: Vec<(String, PathBuf)>,
        },
        Visited(PathBuf),
        Focus(bool),
        Flush(Sender<()>),
        Shutdown,
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
            let (tx, rx) = unbounded();
            let machine = Machine::new(Arc::clone(&shared), options, tx.clone());
            let threads = vec![
                spawn("size-index", move || machine.run(&rx)),
                spawn("size-index-scan", {
                    let (shared, tx) = (Arc::clone(&shared), tx.clone());
                    move || run_background(&shared, &tx)
                }),
                spawn("size-index-boost", {
                    let (shared, tx) = (Arc::clone(&shared), tx.clone());
                    move || run_boosts(&shared, &tx)
                }),
            ];
            Self {
                shared,
                tx,
                threads,
            }
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

        /// Answers indexed paths at once (FR3.1); the rest are boosted and
        /// answered through `emitter` when their walk completes (FR3.2).
        pub fn request(&self, paths: Vec<String>, mode: RequestMode, emitter: SizeEmitter) {
            if !self.shared.booted.load(Ordering::Acquire) {
                let _ = self.tx.send(Msg::Request {
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
            let mut cancelled = before - lanes.boosts.len();
            if let Some((path, cancel)) = &lanes.running_boost {
                if paths.contains(path) {
                    cancel.store(true, Ordering::Relaxed);
                    cancelled += 1;
                }
            }
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
                let _ = self.tx.send(Msg::Visited(root));
            }
            let folders = folders
                .into_iter()
                .map(|folder| {
                    let tracked = self.tracked(Path::new(&folder));
                    (folder, tracked)
                })
                .collect();
            let _ = self.tx.send(Msg::Subscribe { pane, folders });
        }

        /// The normalized path `path`'s total lives at. Only indexed paths
        /// are resolved, so other volumes and shares are never touched.
        fn tracked(&self, path: &Path) -> PathBuf {
            if !self.contains(path) {
                return platform::normalize(path);
            }
            match route_of(path) {
                Route::Target(target) => platform::normalize(&target),
                Route::AsIs | Route::NotDir => platform::normalize(path),
            }
        }

        pub fn focus_changed(&self, focused: bool) {
            let _ = self.tx.send(Msg::Focus(focused));
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
            if self.tx.send(Msg::Flush(ack)).is_ok() {
                let _ = done.recv_timeout(FLUSH_TIMEOUT);
            }
        }
    }

    impl Drop for FolderSizeIndex {
        fn drop(&mut self) {
            {
                let mut lanes = self.shared.lanes();
                lanes.shutdown = true;
                let running = lanes
                    .running_background
                    .iter()
                    .chain(lanes.running_boost.iter().map(|(_, cancel)| cancel));
                for cancel in running {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            self.shared.wake.notify_all();
            let _ = self.tx.send(Msg::Shutdown);
            for thread in self.threads.drain(..) {
                let _ = thread.join();
            }
        }
    }

    fn spawn(name: &str, body: impl FnOnce() + Send + 'static) -> JoinHandle<()> {
        thread::Builder::new()
            .name(name.to_string())
            .spawn(body)
            .expect("spawn size index thread")
    }

    impl Shared {
        fn request(&self, paths: Vec<String>, mode: RequestMode, emitter: SizeEmitter) {
            let mut answered = Vec::new();
            let mut boosted = Vec::new();
            {
                let view = self.view();
                for path in paths {
                    match self.answer(&view, &path, mode) {
                        Some(update) => answered.push(update),
                        None => {
                            answered.push(SizeUpdate::new(
                                path.clone(),
                                SizeStateKind::Calculating,
                                SizeSource::Index,
                                None,
                            ));
                            boosted.push(path);
                        }
                    }
                }
            }
            if !answered.is_empty() {
                emitter(answered);
            }
            if !boosted.is_empty() {
                self.queue_boosts(boosted, mode == RequestMode::Refresh, Some(emitter));
            }
        }

        fn view(&self) -> std::sync::RwLockReadGuard<'_, View> {
            self.view.read().expect("index view lock")
        }

        fn view_mut(&self) -> std::sync::RwLockWriteGuard<'_, View> {
            self.view.write().expect("index view lock")
        }

        fn lanes(&self) -> MutexGuard<'_, Lanes> {
            self.lanes.lock().expect("index lanes lock")
        }

        /// Inside a root and outside every exclusion (`path` normalized).
        fn indexes(&self, path: &Path) -> bool {
            self.roots.iter().any(|root| within(path, root))
                && !self.excluded.iter().any(|excluded| within(path, excluded))
        }

        fn protected_root_of(&self, path: &Path) -> Option<&PathBuf> {
            self.protected.iter().find(|root| within(path, root))
        }

        /// Background and automatic work must not read `path` (FR5.1).
        fn privacy_blocked(&self, view: &View, path: &Path) -> bool {
            !view.full_disk_access
                && self
                    .protected_root_of(path)
                    .is_some_and(|root| !view.visited.contains(root))
        }

        /// Marks the protected root containing `path` visited; returns it if
        /// it was not visited before.
        fn visit(&self, path: &Path) -> Option<PathBuf> {
            let root = self.protected_root_of(&platform::normalize(path))?.clone();
            self.view_mut().visited.insert(root.clone()).then_some(root)
        }

        /// `None` means the path needs a boosted walk.
        fn answer(&self, view: &View, path: &str, mode: RequestMode) -> Option<SizeUpdate> {
            let normalized = platform::normalize(Path::new(path));
            if self.privacy_blocked(view, &normalized) {
                return Some(unknown(path));
            }
            let total = view.tree.total(&normalized)?;
            (mode != RequestMode::Refresh && !total.skipped && total.fully_indexed)
                .then(|| update_from_total(path, &total))
        }

        fn queue_boosts(&self, paths: Vec<String>, refresh: bool, waiter: Option<SizeEmitter>) {
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
        fn queue_background(&self, root: PathBuf, full: bool) -> bool {
            let mut lanes = self.lanes();
            if lanes.background.iter().any(|job| job.root == root) {
                return false;
            }
            lanes.background.push_back(ScanJob { root, full });
            self.wake.notify_all();
            true
        }

        fn walk_options(&self, cancel: Arc<AtomicBool>, qos: Qos, threads: usize) -> WalkOptions {
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
    fn within(path: &Path, root: &Path) -> bool {
        path.starts_with(root) || lowercase(path).starts_with(lowercase(root))
    }

    fn lowercase(path: &Path) -> PathBuf {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }

    fn unknown(path: &str) -> SizeUpdate {
        SizeUpdate::new(
            path.to_string(),
            SizeStateKind::Unknown,
            SizeSource::Index,
            None,
        )
    }

    fn update_from_total(path: &str, total: &FolderTotal) -> SizeUpdate {
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

    fn unix_secs(time: SystemTime) -> u64 {
        time.duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
    }

    // ----- walk lanes -------------------------------------------------------

    fn run_background(shared: &Shared, tx: &Sender<Msg>) {
        loop {
            let (job, cancel) = {
                let mut lanes = shared.lanes();
                loop {
                    if lanes.shutdown {
                        return;
                    }
                    if !lanes.background_must_wait() {
                        if let Some(job) = lanes.background.pop_front() {
                            let cancel = Arc::new(AtomicBool::new(false));
                            lanes.running_background = Some(Arc::clone(&cancel));
                            break (job, cancel);
                        }
                    }
                    lanes = shared.wake.wait(lanes).expect("index lanes lock");
                }
            };
            let outcome = walk(
                shared,
                tx,
                &job.root,
                Lane::Background { full: job.full },
                &cancel,
            );
            shared.lanes().running_background = None;
            let _ = tx.send(Msg::ScanDone { job, outcome });
        }
    }

    fn run_boosts(shared: &Shared, tx: &Sender<Msg>) {
        loop {
            let (job, cancel) = {
                let mut lanes = shared.lanes();
                loop {
                    if lanes.shutdown {
                        return;
                    }
                    if let Some(job) = lanes.boosts.pop_front() {
                        let cancel = Arc::new(AtomicBool::new(false));
                        lanes.running_boost = Some((job.path.clone(), Arc::clone(&cancel)));
                        break (job, cancel);
                    }
                    lanes = shared.wake.wait(lanes).expect("index lanes lock");
                }
            };
            let outcome = match shared.coordinator.submit(JobSpec::new(
                [JobClass::Throughput],
                ["size-index-boost".to_string()],
            )) {
                Ok(_admission) => walk(
                    shared,
                    tx,
                    &platform::normalize(Path::new(&job.path)),
                    Lane::Boost {
                        refresh: job.refresh,
                    },
                    &cancel,
                ),
                Err(_) => WalkOutcome::Failed(ManualSizeError::Cancelled),
            };
            shared.lanes().running_boost = None;
            shared.wake.notify_all();
            let _ = tx.send(Msg::BoostDone { job, outcome });
        }
    }

    #[derive(Clone, Copy)]
    enum Lane {
        /// Gentle: one thread, background QoS. `full` counts as indexing.
        Background { full: bool },
        /// Four threads, user-initiated QoS. Unless refreshing, subtrees the
        /// index already covers are skipped.
        Boost { refresh: bool },
    }

    /// Walks `root`, streaming record batches to the coordination thread.
    fn walk(
        shared: &Shared,
        tx: &Sender<Msg>,
        root: &Path,
        lane: Lane,
        cancel: &Arc<AtomicBool>,
    ) -> WalkOutcome {
        let (mut options, full, skip_indexed) = match lane {
            Lane::Background { full } => (
                shared.walk_options(Arc::clone(cancel), Qos::Background, 1),
                full,
                false,
            ),
            Lane::Boost { refresh } => (
                shared.walk_options(Arc::clone(cancel), Qos::UserInitiated, MAX_JOB_THREADS),
                false,
                !refresh,
            ),
        };
        {
            let view = shared.view();
            if shared.privacy_blocked(&view, root) {
                return WalkOutcome::Skipped;
            }
            if !view.full_disk_access {
                options.excluded.extend(
                    shared
                        .protected
                        .iter()
                        .filter(|root| !view.visited.contains(*root))
                        .cloned(),
                );
            }
            if skip_indexed {
                if view
                    .tree
                    .total(root)
                    .is_some_and(|total| total.fully_indexed && !total.skipped)
                {
                    return WalkOutcome::Skipped;
                }
                options
                    .excluded
                    .extend(view.tree.indexed_subtrees_under(root));
            }
        }

        let batch = Mutex::new(Vec::with_capacity(RECORD_BATCH));
        let flush = |batch: &mut Vec<DirRecord>| {
            if !batch.is_empty() {
                let records = std::mem::take(batch);
                let _ = tx.send(Msg::Records { records, full });
            }
        };
        let gentle = matches!(lane, Lane::Background { .. })
            .then(|| Mutex::new(Gentle::admit(shared, cancel)));
        let result = fast_walk::subtree_records(root, &options, |record| {
            let mut pending = batch.lock().expect("record batch lock");
            pending.push(record);
            if pending.len() >= RECORD_BATCH {
                flush(&mut pending);
            }
            if let Some(gentle) = &gentle {
                gentle
                    .lock()
                    .expect("gentle walk lock")
                    .after_record(shared, cancel, || flush(&mut pending));
            }
        });
        flush(&mut batch.lock().expect("record batch lock"));
        match result {
            Ok(()) => WalkOutcome::Walked,
            Err(error) => WalkOutcome::Failed(error),
        }
    }

    /// Background-walk manners: hold Throughput admission for at most
    /// [`READMIT_EVERY_DIRS`] directories, and step aside (releasing it)
    /// while the power gate is closed or a boost waits.
    struct Gentle {
        admission: Option<JobHandle>,
        since_admitted: u64,
    }

    impl Gentle {
        fn admit(shared: &Shared, cancel: &AtomicBool) -> Self {
            let admission = shared
                .coordinator
                .submit(JobSpec::new(
                    [JobClass::Throughput],
                    ["size-index".to_string()],
                ))
                .ok();
            if admission.is_none() {
                cancel.store(true, Ordering::Relaxed);
            }
            Self {
                admission,
                since_admitted: 0,
            }
        }

        fn after_record(&mut self, shared: &Shared, cancel: &AtomicBool, flush: impl FnOnce()) {
            self.since_admitted += 1;
            let must_wait = shared.lanes().background_must_wait();
            if self.since_admitted < READMIT_EVERY_DIRS && !must_wait {
                return;
            }
            flush();
            self.admission = None;
            {
                let mut lanes = shared.lanes();
                while lanes.background_must_wait() && !lanes.shutdown {
                    #[cfg(feature = "test-utils")]
                    (shared.on_background_parked)();
                    lanes = shared.wake.wait(lanes).expect("index lanes lock");
                }
            }
            *self = Self::admit(shared, cancel);
        }
    }

    // ----- coordination thread ---------------------------------------------

    struct FullScan {
        /// The event ID captured before the scan (FR2.2).
        captured_id: u64,
        remaining: usize,
    }

    enum Phase {
        Booting,
        /// Replaying history since the snapshot's event ID.
        CatchUp {
            saved: SnapshotMeta,
            current_id: u64,
            events: Vec<ChangeEvent>,
        },
        Live,
    }

    struct Machine {
        shared: Arc<Shared>,
        tx: Sender<Msg>,
        platform: Arc<dyn IndexPlatform>,
        clock: Clock,
        emit_sizes: SizeEmitter,
        emit_status: StatusEmitter,
        snapshot_path: PathBuf,
        stream_excluded: Vec<PathBuf>,
        stream: Option<StreamHandle>,
        phase: Phase,
        meta: SnapshotMeta,
        meta_dirty: bool,
        last_applied_id: u64,
        full: Option<FullScan>,
        background_inflight: usize,
        items_scanned: u64,
        subscriptions: HashMap<String, Vec<(String, PathBuf)>>,
        sleeping: bool,
        low_power: bool,
        thermal_pressure: bool,
        last_progress_emit: Option<SystemTime>,
    }

    impl Machine {
        fn new(shared: Arc<Shared>, options: IndexOptions, tx: Sender<Msg>) -> Self {
            Self {
                shared,
                tx,
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

        fn run(mut self, rx: &Receiver<Msg>) {
            (self.emit_status)(
                self.shared
                    .status
                    .lock()
                    .expect("index status lock")
                    .clone(),
            );
            self.boot();
            self.after_change();
            while let Ok(msg) = rx.recv() {
                match msg {
                    Msg::Shutdown => break,
                    msg => self.handle(msg),
                }
                self.after_change();
            }
        }

        fn boot(&mut self) {
            let power_tx = self.tx.clone();
            let conditions = self.platform.observe_power(Arc::new(move |signal| {
                let _ = power_tx.send(Msg::Power(signal));
            }));
            self.power(conditions);

            // FSEvents history for user data lives on the Data volume; on
            // macOS 10.15–13 `/` itself is the sealed system volume.
            let uuid = self
                .platform
                .volume_uuid(Path::new(platform::DATA_VOLUME_PREFIX))
                .unwrap_or_default();
            let os_build = self.platform.os_build().unwrap_or_default();
            let current_id = self.platform.current_event_id();
            let snapshot = persist::load(&self.snapshot_path);
            {
                let mut view = self.shared.view_mut();
                view.full_disk_access = self.platform.full_disk_access();
                if let Some(snapshot) = &snapshot {
                    view.visited
                        .extend(snapshot.meta.visited_roots.iter().cloned());
                }
            }
            let since = match snapshot {
                Some(snapshot)
                    if snapshot.meta.volume_uuid == uuid
                        && snapshot.meta.os_build == os_build
                        && snapshot.meta.event_id <= current_id =>
                {
                    let saved = snapshot.meta.clone();
                    self.load_tree(snapshot.tree);
                    self.meta = snapshot.meta;
                    self.last_applied_id = saved.event_id;
                    self.phase = Phase::CatchUp {
                        saved,
                        current_id,
                        events: Vec::new(),
                    };
                    Some(self.meta.event_id)
                }
                stale => {
                    // Missing, corrupt, foreign, from another OS build or
                    // ahead: rescan, keeping any old totals visible meanwhile
                    // (FR2.5).
                    if let Some(snapshot) = stale {
                        self.load_tree(snapshot.tree);
                    }
                    self.meta = SnapshotMeta {
                        volume_uuid: uuid,
                        os_build,
                        ..SnapshotMeta::default()
                    };
                    self.last_applied_id = current_id;
                    self.phase = Phase::Live;
                    None
                }
            };
            self.apply_privacy();
            self.shared.booted.store(true, Ordering::Release);

            let tx = self.tx.clone();
            self.stream = self.platform.start_stream(
                &self.shared.roots,
                &self.stream_excluded,
                since,
                Arc::new(move |events| {
                    let _ = tx.send(Msg::Events(events));
                }),
            );
            if self.stream.is_none() {
                self.shared.view_mut().available = false;
                self.shared.lanes().background.clear();
                return;
            }
            if since.is_none() || self.weekly_due() {
                self.start_full_scan();
            } else {
                let tops = self.shared.view().tree.unscanned_tops();
                for top in tops {
                    self.queue_background(top);
                }
            }
        }

        fn load_tree(&mut self, mut tree: FolderSizeTree) {
            for root in &self.shared.roots {
                tree.add_root(root);
            }
            self.shared.view_mut().tree = tree;
        }

        fn handle(&mut self, msg: Msg) {
            match msg {
                Msg::Events(events) => self.events(events),
                Msg::Power(signal) => self.power(signal),
                Msg::Records { records, full } => self.records(records, full),
                Msg::ScanDone { job, outcome } => self.scan_done(job, outcome),
                Msg::BoostDone { job, outcome } => self.boost_done(job, outcome),
                Msg::Request {
                    paths,
                    mode,
                    emitter,
                } => self.shared.request(paths, mode, emitter),
                Msg::Subscribe { pane, folders } => self.subscribe(pane, folders),
                Msg::Visited(root) => self.visited(root),
                Msg::Focus(true) => self.focused(),
                Msg::Focus(false) => self.save_snapshot(),
                Msg::Flush(ack) => {
                    self.save_snapshot();
                    let _ = ack.send(());
                }
                Msg::Shutdown => {}
            }
        }

        // ----- events --------------------------------------------------------

        fn events(&mut self, raw: Vec<RawEvent>) {
            let mut live = Vec::new();
            let mut history_done = false;
            let mut max_id = 0;
            for event in raw {
                if event.history_done {
                    history_done = true;
                    continue;
                }
                max_id = max_id.max(event.id);
                let flags = event.flags;
                let path = platform::normalize(&event.path);
                let full = flags.user_dropped
                    || flags.kernel_dropped
                    || flags.event_ids_wrapped
                    || flags.root_changed;
                // Drops other volumes, devfs and the non-Data parts of
                // `/System/Volumes`; Data-prefixed paths were normalized.
                if full || self.shared.indexes(&path) {
                    live.push(ChangeEvent { path, flags });
                }
            }
            match std::mem::replace(&mut self.phase, Phase::Live) {
                Phase::CatchUp {
                    saved,
                    current_id,
                    mut events,
                } => {
                    events.append(&mut live);
                    if !history_done {
                        self.phase = Phase::CatchUp {
                            saved,
                            current_id,
                            events,
                        };
                        return;
                    }
                    let (volume_uuid, os_build) =
                        (self.meta.volume_uuid.clone(), self.meta.os_build.clone());
                    self.apply_plan(persist::plan_catch_up(
                        &saved,
                        &volume_uuid,
                        &os_build,
                        current_id,
                        &events,
                    ));
                    self.last_applied_id = self.last_applied_id.max(max_id).max(current_id);
                    self.save_snapshot();
                }
                phase => {
                    self.phase = phase;
                    self.apply_plan(persist::plan_batch(&live));
                    self.last_applied_id = self.last_applied_id.max(max_id);
                }
            }
        }

        fn apply_plan(&mut self, outcome: ReplayOutcome) {
            match outcome {
                ReplayOutcome::UpToDate => {}
                ReplayOutcome::FullRescan => self.start_full_scan(),
                ReplayOutcome::Apply { reread, rescan } => {
                    for dir in reread {
                        self.reread(&dir);
                    }
                    for root in rescan {
                        self.queue_background(root);
                    }
                }
            }
        }

        /// Re-reads one reported directory (FR2.3). Directories a scan has
        /// not reached yet, and unvisited protected ones, are left alone.
        fn reread(&mut self, dir: &Path) {
            {
                let view = self.shared.view();
                if self.shared.privacy_blocked(&view, dir)
                    || !view.tree.total(dir).is_some_and(|total| total.scanned)
                {
                    return;
                }
            }
            let options =
                self.shared
                    .walk_options(Arc::new(AtomicBool::new(false)), Qos::UserInitiated, 1);
            let added = match fast_walk::list_dir(dir, &options) {
                Ok(listing) => self.shared.view_mut().tree.apply_listing(
                    dir,
                    listing.direct_bytes,
                    &listing.subdirs,
                    listing.denied,
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
                if self.shared.privacy_blocked(&self.shared.view(), &path) {
                    continue;
                }
                let display = path.to_string_lossy().into_owned();
                if subscribed.iter().any(|(_, normalized)| *normalized == path) {
                    self.shared.queue_boosts(vec![display], false, None);
                } else {
                    self.queue_background(path);
                }
            }
        }

        fn prune(&mut self, dir: &Path) {
            if !self.shared.roots.iter().any(|root| root == dir) {
                self.shared.view_mut().tree.prune(dir);
            }
        }

        // ----- scans ---------------------------------------------------------

        fn start_full_scan(&mut self) {
            if self.full.is_some() {
                return;
            }
            self.items_scanned = 0;
            self.full = Some(FullScan {
                captured_id: self.platform.current_event_id(),
                remaining: 0,
            });
            for root in self.shared.roots.clone() {
                if self.shared.queue_background(root, true) {
                    self.background_inflight += 1;
                    if let Some(full) = &mut self.full {
                        full.remaining += 1;
                    }
                }
            }
            if self.full.as_ref().is_some_and(|full| full.remaining == 0) {
                self.finish_full_scan();
            }
        }

        fn queue_background(&mut self, root: PathBuf) {
            if self.shared.queue_background(root, false) {
                self.background_inflight += 1;
            }
        }

        fn records(&mut self, records: Vec<DirRecord>, full: bool) {
            {
                let mut view = self.shared.view_mut();
                for record in &records {
                    view.tree.merge_record(
                        &record.path,
                        record.direct_bytes,
                        &record.subdirs,
                        record.denied,
                    );
                }
            }
            self.reskip_protected();
            if full {
                // Items are folders plus their files (FR7.1).
                self.items_scanned += records
                    .iter()
                    .map(|record| 1 + record.file_count)
                    .sum::<u64>();
            }
        }

        fn scan_done(&mut self, job: ScanJob, outcome: WalkOutcome) {
            self.background_inflight = self.background_inflight.saturating_sub(1);
            self.walk_finished(&job.root, &outcome);
            if job.full {
                if let Some(full) = &mut self.full {
                    full.remaining = full.remaining.saturating_sub(1);
                    if full.remaining == 0 {
                        self.finish_full_scan();
                    }
                }
            }
        }

        fn finish_full_scan(&mut self) {
            if let Some(full) = self.full.take() {
                // Saved as is, even if later events were applied (FR2.2).
                self.meta.event_id = full.captured_id;
                self.last_applied_id = full.captured_id;
                self.meta.last_full_scan_secs = unix_secs((self.clock)());
                self.meta_dirty = true;
                self.save_snapshot();
            }
        }

        fn boost_done(&mut self, job: BoostJob, outcome: WalkOutcome) {
            let path = platform::normalize(Path::new(&job.path));
            self.walk_finished(&path, &outcome);
            let update = match &outcome {
                WalkOutcome::Failed(ManualSizeError::Cancelled) => return,
                WalkOutcome::Failed(_) => SizeUpdate::new(
                    job.path.clone(),
                    SizeStateKind::Error,
                    SizeSource::Index,
                    None,
                ),
                WalkOutcome::Walked | WalkOutcome::Skipped => {
                    let view = self.shared.view();
                    if self.shared.privacy_blocked(&view, &path) {
                        unknown(&job.path)
                    } else {
                        view.tree.total(&path).map_or_else(
                            || {
                                SizeUpdate::new(
                                    job.path.clone(),
                                    SizeStateKind::Error,
                                    SizeSource::Index,
                                    None,
                                )
                            },
                            |total| update_from_total(&job.path, &total),
                        )
                    }
                }
            };
            for waiter in job.waiters {
                waiter(vec![update.clone()]);
            }
        }

        fn walk_finished(&mut self, root: &Path, outcome: &WalkOutcome) {
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

        // ----- privacy -------------------------------------------------------

        /// Startup privacy state: unvisited protected roots are skipped (and
        /// stale data under them dropped) unless Full Disk Access is granted.
        fn apply_privacy(&mut self) {
            let mut view = self.shared.view_mut();
            for root in &self.shared.protected {
                let blocked = !view.full_disk_access && !view.visited.contains(root);
                let node = view.tree.total(root);
                if blocked {
                    if node.as_ref().is_some_and(|total| total.scanned) {
                        view.tree.prune(root);
                    }
                    if root.symlink_metadata().is_ok_and(|meta| meta.is_dir()) {
                        view.tree.set_skipped(root, true);
                    }
                } else if node.is_some_and(|total| total.skipped) {
                    view.tree.set_skipped(root, false);
                }
            }
        }

        /// A protected root that appeared in a listing while unvisited is
        /// skipped, so walks never reach it and ancestors report a bound.
        fn reskip_protected(&mut self) {
            let mut view = self.shared.view_mut();
            if view.full_disk_access {
                return;
            }
            for root in &self.shared.protected {
                if !view.visited.contains(root)
                    && view
                        .tree
                        .total(root)
                        .is_some_and(|total| !total.skipped && !total.scanned)
                {
                    view.tree.set_skipped(root, true);
                }
            }
        }

        fn visited(&mut self, root: PathBuf) {
            self.shared.view_mut().tree.set_skipped(&root, false);
            self.meta_dirty = true;
            self.queue_background(root);
        }

        fn focused(&mut self) {
            let granted = self.platform.full_disk_access();
            let lifted = {
                let mut view = self.shared.view_mut();
                let newly = granted && !view.full_disk_access;
                view.full_disk_access = granted;
                if newly {
                    let lifted: Vec<PathBuf> = self
                        .shared
                        .protected
                        .iter()
                        .filter(|root| !view.visited.contains(*root))
                        .cloned()
                        .collect();
                    for root in &lifted {
                        view.tree.set_skipped(root, false);
                    }
                    lifted
                } else {
                    Vec::new()
                }
            };
            for root in lifted {
                self.queue_background(root);
            }
            if matches!(self.phase, Phase::Live) && self.stream.is_some() && self.weekly_due() {
                self.start_full_scan();
            }
        }

        fn weekly_due(&self) -> bool {
            let last = UNIX_EPOCH + Duration::from_secs(self.meta.last_full_scan_secs);
            (self.clock)()
                .duration_since(last)
                .is_ok_and(|age| age > WEEK)
        }

        // ----- subscriptions -------------------------------------------------

        fn subscribed(&self) -> Vec<(String, PathBuf)> {
            let mut seen = HashSet::new();
            self.subscriptions
                .values()
                .flatten()
                .filter(|(path, _)| seen.insert(path.as_str()))
                .cloned()
                .collect()
        }

        /// Replaces a pane's set; newly subscribed folders get their current
        /// indexed totals, so a revisited folder is never stale.
        fn subscribe(&mut self, pane: String, folders: Vec<(String, PathBuf)>) {
            let before: HashSet<String> = self
                .subscribed()
                .into_iter()
                .map(|(path, _)| path)
                .collect();
            if folders.is_empty() {
                self.subscriptions.remove(&pane);
            } else {
                self.subscriptions.insert(pane, folders);
            }
            let view = self.shared.view();
            let updates: Vec<SizeUpdate> = self
                .subscribed()
                .into_iter()
                .filter(|(path, _)| !before.contains(path))
                .filter_map(|(path, normalized)| {
                    let total = view.tree.total(&normalized)?;
                    (!self.shared.privacy_blocked(&view, &normalized)
                        && total.fully_indexed
                        && !total.skipped)
                        .then(|| update_from_total(&path, &total))
                })
                .collect();
            drop(view);
            if !updates.is_empty() {
                (self.emit_sizes)(updates);
            }
        }

        /// After every message: emit changed subscribed totals (coalesced per
        /// batch) and the status.
        fn after_change(&mut self) {
            let subscribed = self.subscribed();
            let updates: Vec<SizeUpdate> = {
                let mut view = self.shared.view_mut();
                let changed: HashSet<PathBuf> = view
                    .tree
                    .drain_changed(subscribed.iter().map(|(_, normalized)| normalized))
                    .into_iter()
                    .collect();
                subscribed
                    .iter()
                    .filter(|(_, normalized)| changed.contains(normalized))
                    .filter_map(|(path, normalized)| {
                        if self.shared.privacy_blocked(&view, normalized) {
                            return None;
                        }
                        let total = view.tree.total(normalized)?;
                        (total.fully_indexed && !total.skipped)
                            .then(|| update_from_total(path, &total))
                    })
                    .collect()
            };
            if !updates.is_empty() {
                (self.emit_sizes)(updates);
            }
            self.publish_status();
        }

        // ----- power and status ----------------------------------------------

        fn power(&mut self, signal: PowerSignal) {
            match signal {
                PowerSignal::WillSleep => self.sleeping = true,
                PowerSignal::DidWake => self.sleeping = false,
                PowerSignal::Conditions {
                    low_power,
                    thermal_pressure,
                } => {
                    self.low_power = low_power;
                    self.thermal_pressure = thermal_pressure;
                }
            }
            self.shared.lanes().power_paused =
                self.sleeping || self.low_power || self.thermal_pressure;
            self.shared.wake.notify_all();
        }

        fn compute_status(&self) -> FolderSizeIndexStatus {
            if self.stream.is_none() && !matches!(self.phase, Phase::Booting) {
                return FolderSizeIndexStatus::new(
                    IndexBackend::MacIndex,
                    false,
                    IndexState::Unavailable,
                );
            }
            let mut status =
                FolderSizeIndexStatus::new(IndexBackend::MacIndex, true, IndexState::NotReady);
            let background = self.full.is_some() || self.background_inflight > 0;
            let pause_reason = if self.low_power {
                Some(PauseReason::LowPowerMode)
            } else if self.thermal_pressure {
                Some(PauseReason::Thermal)
            } else {
                None
            };
            status.state = match (&self.phase, pause_reason) {
                (Phase::Booting, _) => IndexState::NotReady,
                (_, Some(reason)) if background => {
                    status.pause_reason = Some(reason);
                    IndexState::Paused
                }
                _ if self.full.is_some() => {
                    status.items_scanned = Some(self.items_scanned);
                    IndexState::Indexing
                }
                (Phase::CatchUp { .. }, _) => IndexState::Updating,
                _ if background => IndexState::Updating,
                _ => IndexState::UpToDate,
            };
            status
        }

        /// Emits on every transition; item-count progress at most once per
        /// [`PROGRESS_INTERVAL`] of the injected clock (FR7.4).
        fn publish_status(&mut self) {
            let next = self.compute_status();
            let mut current = self.shared.status.lock().expect("index status lock");
            if *current == next {
                return;
            }
            let progress_only = FolderSizeIndexStatus {
                items_scanned: current.items_scanned,
                ..next.clone()
            } == *current;
            let now = (self.clock)();
            if progress_only
                && self.last_progress_emit.is_some_and(|last| {
                    now.duration_since(last)
                        .map_or(true, |age| age < PROGRESS_INTERVAL)
                })
            {
                return;
            }
            self.last_progress_emit = Some(now);
            *current = next.clone();
            drop(current);
            (self.emit_status)(next);
        }

        // ----- snapshot ------------------------------------------------------

        /// Writes the snapshot if anything changed (FR2.6). The saved event ID
        /// is the pre-scan one while a full scan runs, and stays put while
        /// subtree scans are in flight, so a relaunch replays what they cover.
        fn save_snapshot(&mut self) {
            if self.stream.is_none() || matches!(self.phase, Phase::Booting) {
                return;
            }
            let event_id = match &self.full {
                Some(full) => full.captured_id,
                None if self.background_inflight > 0
                    || matches!(self.phase, Phase::CatchUp { .. }) =>
                {
                    self.meta.event_id
                }
                None => self.last_applied_id,
            };
            let mut view = self.shared.view_mut();
            if !view.tree.is_dirty() && !self.meta_dirty && event_id == self.meta.event_id {
                return;
            }
            self.meta.event_id = event_id;
            let mut visited: Vec<PathBuf> = view.visited.iter().cloned().collect();
            visited.sort();
            self.meta.visited_roots = visited;
            match persist::save(&self.snapshot_path, &self.meta, &view.tree) {
                Ok(()) => {
                    view.tree.mark_clean();
                    self.meta_dirty = false;
                }
                Err(error) => log::warn!(
                    "size index: snapshot write to {} failed: {error}",
                    self.snapshot_path.display()
                ),
            }
        }
    }
}
