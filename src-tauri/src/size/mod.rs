mod batch;
pub mod everything;
#[cfg(target_os = "macos")]
pub mod fast_walk;
pub mod index;
pub mod manual;
mod route;
pub mod scheduler;
mod worker_pool;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::resource_coordinator::{max_throughput_slots, ResourceCoordinator};
use crate::volumes::{self, VolumeInfo};

use everything::{EverythingAvailability, EverythingHandle};
#[cfg(target_os = "macos")]
pub(crate) use route::{route_of, Route};
use worker_pool::{spawn_manual_worker, QueuedWork};

type UpdateEmitter = Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>;

/// Long-lived manual-size worker threads. A handful of workers pull queued
/// jobs from the shared [`scheduler::SizeScheduler`] one at a time instead of
/// a thread being spawned per requested path — that is what actually bounds
/// how many OS threads a huge selection (or an "Items" sort over a folder
/// with tens of thousands of children) can create. The coordinator's
/// `Throughput` lane (see [`resource_coordinator::max_throughput_slots`])
/// still gates how many of these workers can be doing filesystem work at
/// once; this pool only bounds how many *threads exist*, not how many run
/// concurrently. On macOS each job's traversal is itself parallel
/// (`fast_walk`), but every job draws its extra threads from one
/// process-wide helper cap, so total walk threads stay bounded.
pub const DEFAULT_MANUAL_SIZE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SizeStateKind {
    Unknown,
    Calculating,
    Ready,
    Error,
    Na,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SizeSource {
    Everything,
    Manual,
    Network,
    Index,
}

/// Why a size is `na`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NaReason {
    Network,
    Timeout,
    Denied,
}

/// `auto` comes from automatic sizing (macOS only sizes indexed paths for
/// it), `refresh` forces fresh work, `manual` is an explicit request.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RequestMode {
    Auto,
    #[default]
    Manual,
    Refresh,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IndexBackend {
    Everything,
    MacIndex,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IndexState {
    Unavailable,
    NotReady,
    Indexing,
    Updating,
    UpToDate,
    Paused,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PauseReason {
    LowPowerMode,
    Thermal,
}

/// Backend-neutral folder-size capability and progress.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderSizeIndexStatus {
    pub backend: IndexBackend,
    pub is_available: bool,
    pub state: IndexState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items_scanned: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<PauseReason>,
}

impl FolderSizeIndexStatus {
    pub fn new(backend: IndexBackend, is_available: bool, state: IndexState) -> Self {
        Self {
            backend,
            is_available,
            state,
            items_scanned: None,
            pause_reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SizeUpdate {
    pub path: String,
    pub state: SizeStateKind,
    pub source: SizeSource,
    pub size_bytes: Option<u64>,
    pub na_reason: Option<NaReason>,
    /// The total excludes skipped protected or denied folders below it.
    pub is_lower_bound: bool,
    /// Up to three skipped protected folders' names.
    pub skipped_roots: Option<Vec<String>>,
    /// Skipped protected folders beyond `skipped_roots`.
    pub skipped_more: Option<u32>,
}

impl SizeUpdate {
    fn new(path: String, state: SizeStateKind, source: SizeSource) -> Self {
        Self {
            path,
            state,
            source,
            size_bytes: None,
            na_reason: None,
            is_lower_bound: false,
            skipped_roots: None,
            skipped_more: None,
        }
    }

    pub fn without_size(path: String, state: SizeStateKind, source: SizeSource) -> Self {
        Self::new(path, state, source)
    }

    pub fn ready(path: String, source: SizeSource, size_bytes: u64) -> Self {
        Self {
            size_bytes: Some(size_bytes),
            ..Self::new(path, SizeStateKind::Ready, source)
        }
    }

    pub fn na(path: String, source: SizeSource, reason: Option<NaReason>) -> Self {
        Self {
            na_reason: reason,
            ..Self::new(path, SizeStateKind::Na, source)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeRequest {
    pub paths: Vec<String>,
    pub mode: RequestMode,
}

impl SizeRequest {
    pub fn manual(paths: Vec<String>) -> Self {
        Self {
            paths,
            mode: RequestMode::Manual,
        }
    }
}

pub struct SizeRequestWithVolumes {
    pub request: SizeRequest,
    pub volumes: Vec<VolumeInfo>,
}

/// One pane's live-update set: `folders` receive `size://state` whenever
/// their indexed totals change, and `current_path` marks a protected folder
/// as visited.
pub struct PaneSubscriptions {
    pub pane_id: String,
    pub current_path: Option<String>,
    pub folders: Vec<String>,
}

struct ServiceParts {
    timeout: Duration,
    everything: Option<Arc<EverythingHandle>>,
    coordinator: Arc<ResourceCoordinator>,
    scheduler_capacity: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeBackend {
    Everything,
    #[cfg(target_os = "macos")]
    Index,
    Manual,
    NetworkNa,
}

pub struct SizeService {
    inner: Arc<Mutex<SizeServiceInner>>,
    timeout: Duration,
    coordinator: Arc<ResourceCoordinator>,
    /// The bounded manual-size worker pool. Joined deterministically by
    /// `Drop` once the scheduler is shut down (which wakes every worker
    /// blocked in `scheduler.next()`).
    workers: Mutex<Vec<JoinHandle<()>>>,
}

struct SizeServiceInner {
    jobs: HashMap<String, Arc<AtomicBool>>,
    scheduler: Arc<scheduler::SizeScheduler<QueuedWork>>,
    everything: Option<Arc<EverythingHandle>>,
    /// Attached only by the explicit index constructors below.
    #[cfg(target_os = "macos")]
    index: Option<Arc<index::FolderSizeIndex>>,
}

impl Default for SizeService {
    fn default() -> Self {
        Self::new(DEFAULT_MANUAL_SIZE_TIMEOUT)
    }
}

impl SizeService {
    pub fn new(timeout: Duration) -> Self {
        Self::with_resource_coordinator(timeout, Arc::new(ResourceCoordinator::new()))
    }

    /// Builds the service against the app-wide shared coordinator (mirrors
    /// `OpsService::with_resource_coordinator`) instead of constructing a
    /// private one. `lib.rs`'s setup constructs the shared coordinator once
    /// and injects it into every subsystem that submits admission-gated
    /// work, so folder-size traversal is fairly scheduled against the same
    /// throughput/CPU/latency lanes as copy/move and everything else.
    pub fn with_resource_coordinator(
        timeout: Duration,
        coordinator: Arc<ResourceCoordinator>,
    ) -> Self {
        let everything = EverythingHandle::load().ok().map(Arc::new);
        Self::build(ServiceParts {
            timeout,
            everything,
            coordinator,
            scheduler_capacity: scheduler::MAX_SIZE_METADATA_ENTRIES,
        })
    }

    #[cfg(feature = "test-utils")]
    pub fn with_everything_handle(timeout: Duration, handle: Option<EverythingHandle>) -> Self {
        Self::with_everything_handle_and_coordinator(
            timeout,
            handle,
            Arc::new(ResourceCoordinator::new()),
        )
    }

    /// Test-only constructor that lets a test inject both a fake Everything
    /// handle and a specific coordinator (e.g. the app-wide shared one, to
    /// exercise fairness across subsystems), following the same convention
    /// as `OpsService`'s test-utils constructors.
    #[cfg(feature = "test-utils")]
    pub fn with_everything_handle_and_coordinator(
        timeout: Duration,
        handle: Option<EverythingHandle>,
        coordinator: Arc<ResourceCoordinator>,
    ) -> Self {
        Self::build(ServiceParts {
            timeout,
            everything: handle.map(Arc::new),
            coordinator,
            scheduler_capacity: scheduler::MAX_SIZE_METADATA_ENTRIES,
        })
    }

    /// Test-only constructor that additionally overrides the scheduler's
    /// job-count capacity (production always uses
    /// [`scheduler::MAX_SIZE_METADATA_ENTRIES`]), so a test can deterministically
    /// exercise the "scheduler rejects a new job because it is full" branch
    /// of `register_job` without needing to queue thousands of real paths.
    #[cfg(feature = "test-utils")]
    pub fn with_capacity_for_tests(
        timeout: Duration,
        coordinator: Arc<ResourceCoordinator>,
        capacity: usize,
    ) -> Self {
        Self::build(ServiceParts {
            timeout,
            everything: None,
            coordinator,
            scheduler_capacity: capacity,
        })
    }

    fn build(parts: ServiceParts) -> Self {
        let ServiceParts {
            timeout,
            everything,
            coordinator,
            scheduler_capacity,
        } = parts;
        let scheduler = Arc::new(scheduler::SizeScheduler::new(scheduler_capacity));
        let inner = Arc::new(Mutex::new(SizeServiceInner {
            jobs: HashMap::new(),
            scheduler: Arc::clone(&scheduler),
            everything,
            #[cfg(target_os = "macos")]
            index: None,
        }));

        let workers = (0..max_throughput_slots())
            .map(|_| {
                spawn_manual_worker(
                    Arc::clone(&scheduler),
                    Arc::clone(&inner),
                    Arc::clone(&coordinator),
                )
            })
            .collect();

        Self {
            inner,
            timeout,
            coordinator,
            workers: Mutex::new(workers),
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn manual_worker_count_for_tests(&self) -> usize {
        self.workers
            .lock()
            .expect("size service workers lock")
            .len()
    }

    /// Production macOS constructor: attaches the folder-size index, whose
    /// snapshot lives in `cache_dir`.
    #[cfg(all(target_os = "macos", not(feature = "test-utils")))]
    pub fn with_folder_size_index(
        timeout: Duration,
        coordinator: Arc<ResourceCoordinator>,
        index_setup: index::MacIndexSetup<'_>,
    ) -> Self {
        let options = index::IndexOptions::for_this_mac(index_setup, Arc::clone(&coordinator));
        let service = Self::with_resource_coordinator(timeout, coordinator);
        service.attach_index(index::FolderSizeIndex::start(options));
        service
    }

    /// Test constructor attaching an index built from injected options
    /// (temp roots, fake platform, clock); never touches real OS services.
    #[cfg(all(target_os = "macos", feature = "test-utils"))]
    pub fn with_folder_size_index_for_tests(
        timeout: Duration,
        options: index::IndexOptions,
    ) -> Self {
        let service = Self::build(ServiceParts {
            timeout,
            everything: None,
            coordinator: Arc::clone(&options.coordinator),
            scheduler_capacity: scheduler::MAX_SIZE_METADATA_ENTRIES,
        });
        service.attach_index(index::FolderSizeIndex::start(options));
        service
    }

    #[cfg(target_os = "macos")]
    fn attach_index(&self, index: index::FolderSizeIndex) {
        self.inner.lock().expect("size service lock").index = Some(Arc::new(index));
    }

    /// The macOS folder-size index, once attached.
    #[cfg(target_os = "macos")]
    pub fn folder_size_index(&self) -> Option<Arc<index::FolderSizeIndex>> {
        self.inner.lock().expect("size service lock").index.clone()
    }

    /// The loaded Everything handle, shared so search reuses the same DLL
    /// and query lock.
    pub fn everything_handle(&self) -> Option<Arc<EverythingHandle>> {
        self.inner
            .lock()
            .expect("size service lock")
            .everything
            .clone()
    }

    /// The neutral folder-size capability: Everything when a handle is
    /// loaded, else the macOS index, else none.
    pub fn folder_size_index_status(&self) -> FolderSizeIndexStatus {
        let availability = self
            .inner
            .lock()
            .expect("size service lock")
            .everything
            .as_ref()
            .map(|handle| handle.availability());
        let everything = |is_available, state| {
            FolderSizeIndexStatus::new(IndexBackend::Everything, is_available, state)
        };
        match availability {
            Some(EverythingAvailability::Available) => everything(true, IndexState::UpToDate),
            Some(EverythingAvailability::NotReady) => everything(false, IndexState::NotReady),
            Some(EverythingAvailability::Unavailable) => everything(false, IndexState::Unavailable),
            None => self.status_without_everything(),
        }
    }

    #[cfg(windows)]
    fn status_without_everything(&self) -> FolderSizeIndexStatus {
        FolderSizeIndexStatus::new(IndexBackend::Everything, false, IndexState::Unavailable)
    }

    #[cfg(target_os = "macos")]
    fn status_without_everything(&self) -> FolderSizeIndexStatus {
        self.folder_size_index()
            .map_or_else(unsupported_status, |index| index.status())
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn status_without_everything(&self) -> FolderSizeIndexStatus {
        unsupported_status()
    }

    pub fn choose_backend_for_path(&self, path: &Path, volumes: &[VolumeInfo]) -> SizeBackend {
        if volumes::path_is_network(path, volumes) {
            return SizeBackend::NetworkNa;
        }

        let inner = self.inner.lock().expect("size service lock");
        if inner
            .everything
            .as_ref()
            .is_some_and(|handle| handle.availability() == EverythingAvailability::Available)
        {
            return SizeBackend::Everything;
        }
        #[cfg(target_os = "macos")]
        if inner
            .index
            .as_ref()
            .is_some_and(|index| index.contains(path))
        {
            return SizeBackend::Index;
        }
        SizeBackend::Manual
    }

    pub fn request_paths<F>(&self, request: SizeRequest, emitter: F)
    where
        F: Fn(Vec<SizeUpdate>) + Send + Sync + 'static,
    {
        let routed = SizeRequestWithVolumes {
            request,
            volumes: volumes::list_volumes(),
        };
        self.request_paths_with_mode(routed, Arc::new(emitter));
    }

    /// Replaces one pane's live-update set.
    pub fn set_subscriptions(&self, subscriptions: PaneSubscriptions) {
        #[cfg(target_os = "macos")]
        if let Some(index) = self.folder_size_index() {
            index.set_subscriptions(subscriptions);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = subscriptions;
    }

    /// Main-window focus changes: focus re-probes Full Disk Access and checks
    /// weekly verification; blur writes the index snapshot.
    pub fn window_focus_changed(&self, focused: bool) {
        #[cfg(target_os = "macos")]
        if let Some(index) = self.folder_size_index() {
            index.focus_changed(focused);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = focused;
    }

    #[cfg(all(target_os = "macos", feature = "test-utils"))]
    pub fn index_lanes_for_tests(&self) -> Option<index::LaneSnapshot> {
        self.folder_size_index()
            .map(|index| index.lanes_for_tests())
    }

    /// Writes the index snapshot before the app exits.
    pub fn flush_index(&self) {
        #[cfg(target_os = "macos")]
        if let Some(index) = self.folder_size_index() {
            index.flush();
        }
    }

    pub fn cancel(&self, path: &str) -> bool {
        self.cancel_many(&[path.to_string()]) > 0
    }

    /// Cancels every in-flight job in `paths`, returning how many were actually
    /// running. Used to abandon a folder's pending size jobs when the pane
    /// navigates away, without disturbing jobs for any other path.
    pub fn cancel_many(&self, paths: &[String]) -> usize {
        let mut inner = self.inner.lock().expect("size service lock");
        let cancelled = paths
            .iter()
            .filter(|path| cancel_tracked_job(&mut inner, path))
            .count();
        #[cfg(target_os = "macos")]
        if let Some(index) = inner.index.clone() {
            drop(inner);
            return cancelled + index.cancel(paths);
        }
        cancelled
    }
}

impl Drop for SizeService {
    fn drop(&mut self) {
        // Wakes every worker parked in `scheduler.next()` (it returns `None`
        // once `shutdown` has been called and the queue is drained), so the
        // pool below joins deterministically instead of leaking blocked
        // threads. The shared `ResourceCoordinator` is intentionally left
        // alone here — unlike `WatchRuntime`, which owns a private
        // coordinator, `SizeService` only holds a reference to the app-wide
        // shared one and must not shut it down out from under other
        // subsystems still using it.
        {
            let inner = self.inner.lock().expect("size service lock");
            inner.scheduler.shutdown();
        }
        for worker in self
            .workers
            .get_mut()
            .expect("size service workers lock")
            .drain(..)
        {
            let _ = worker.join();
        }
    }
}

/// No folder-size backend: macOS without an attached index.
#[cfg(not(windows))]
fn unsupported_status() -> FolderSizeIndexStatus {
    FolderSizeIndexStatus::new(IndexBackend::None, false, IndexState::Unavailable)
}

pub fn size_path_from_string(path: &str) -> PathBuf {
    PathBuf::from(path)
}

fn emit_non_empty(emitter: &UpdateEmitter, updates: Vec<SizeUpdate>) {
    if !updates.is_empty() {
        emitter(updates);
    }
}

fn cancel_tracked_job(inner: &mut SizeServiceInner, path: &str) -> bool {
    let Some(cancel) = inner.jobs.remove(path) else {
        return false;
    };
    cancel.store(true, Ordering::Relaxed);
    inner.scheduler.cancel(path);
    true
}
