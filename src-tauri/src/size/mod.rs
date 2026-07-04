pub mod everything;
pub mod manual;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use rayon::{ThreadPool, ThreadPoolBuilder};
use serde::{Deserialize, Serialize};

use crate::volumes::{self, VolumeInfo};

use everything::{EverythingAvailability, EverythingHandle};

/// Bounded worker pool for manual folder-size jobs.
///
/// Manual sizing spawns one job per folder, so a directory with tens of
/// thousands of subfolders would otherwise create tens of thousands of OS
/// threads — each `stat`ing and `read_dir`ing before it could ever notice a
/// cancellation. Funnelling those jobs through a small pool caps the live
/// thread count and, crucially, lets a cancelled-but-not-yet-started job be
/// skipped the instant a worker picks it up (see [`SizeService::spawn_job`]),
/// so navigating away from a huge folder no longer keeps the CPU pinned while
/// a backlog of queued jobs drains. Each job's `jwalk` walk is itself parallel
/// across the global Rayon pool, so a handful of workers keeps every core busy
/// without oversubscribing.
fn size_pool() -> Option<&'static ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(4)
            .thread_name(|index| format!("fe-size-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}

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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EverythingStatusKind {
    Unsupported,
    Unavailable,
    NotReady,
    Available,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EverythingStatus {
    pub status: EverythingStatusKind,
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SizeUpdate {
    pub path: String,
    pub state: SizeStateKind,
    pub source: SizeSource,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeBackend {
    Everything,
    Manual,
    NetworkNa,
}

pub struct SizeService {
    inner: Arc<Mutex<SizeServiceInner>>,
    timeout: Duration,
}

struct SizeServiceInner {
    jobs: HashMap<String, Arc<AtomicBool>>,
    everything: Option<Arc<EverythingHandle>>,
}

impl Default for SizeService {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl SizeService {
    pub fn new(timeout: Duration) -> Self {
        let everything = EverythingHandle::load().ok().map(Arc::new);

        Self {
            inner: Arc::new(Mutex::new(SizeServiceInner {
                jobs: HashMap::new(),
                everything,
            })),
            timeout,
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn with_everything_handle(timeout: Duration, handle: Option<EverythingHandle>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SizeServiceInner {
                jobs: HashMap::new(),
                everything: handle.map(Arc::new),
            })),
            timeout,
        }
    }

    pub fn everything_status(&self) -> EverythingStatus {
        let inner = self.inner.lock().expect("size service lock");
        let availability = inner
            .everything
            .as_ref()
            .map(|handle| handle.availability());

        match availability {
            Some(EverythingAvailability::Available) => EverythingStatus {
                status: EverythingStatusKind::Available,
                is_available: true,
            },
            #[cfg(windows)]
            Some(EverythingAvailability::NotReady) => EverythingStatus {
                status: EverythingStatusKind::NotReady,
                is_available: false,
            },
            Some(EverythingAvailability::Unavailable) | None if cfg!(windows) => EverythingStatus {
                status: EverythingStatusKind::Unavailable,
                is_available: false,
            },
            _ => EverythingStatus {
                status: EverythingStatusKind::Unsupported,
                is_available: false,
            },
        }
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
            SizeBackend::Everything
        } else {
            SizeBackend::Manual
        }
    }

    pub fn request_paths<F>(&self, paths: Vec<String>, emitter: F)
    where
        F: Fn(Vec<SizeUpdate>) + Send + Sync + 'static,
    {
        let emitter = Arc::new(emitter);
        let volumes = volumes::list_volumes();
        self.request_paths_with_volumes(paths, volumes, emitter);
    }

    pub fn cancel(&self, path: &str) -> bool {
        let mut inner = self.inner.lock().expect("size service lock");
        if let Some(cancel) = inner.jobs.remove(path) {
            cancel.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Cancels every in-flight job in `paths`, returning how many were actually
    /// running. Used to abandon a folder's pending size jobs when the pane
    /// navigates away, without disturbing jobs for any other path.
    pub fn cancel_many(&self, paths: &[String]) -> usize {
        let mut inner = self.inner.lock().expect("size service lock");
        paths
            .iter()
            .filter(|path| {
                inner.jobs.remove(*path).is_some_and(|cancel| {
                    cancel.store(true, Ordering::Relaxed);
                    true
                })
            })
            .count()
    }

    pub fn request_path_with_volumes(
        &self,
        path: String,
        volumes: Vec<VolumeInfo>,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
    ) {
        self.request_paths_with_volumes(vec![path], volumes, emitter);
    }

    pub fn request_paths_with_volumes(
        &self,
        paths: Vec<String>,
        volumes: Vec<VolumeInfo>,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
    ) {
        let mut everything_paths = Vec::new();
        let mut network_updates = Vec::new();

        for path in paths {
            let backend = self.choose_backend_for_path(Path::new(&path), &volumes);

            match backend {
                SizeBackend::NetworkNa => {
                    network_updates.push(SizeUpdate {
                        path,
                        state: SizeStateKind::Na,
                        source: SizeSource::Network,
                        size_bytes: None,
                    });
                }
                SizeBackend::Everything => everything_paths.push(path),
                SizeBackend::Manual => {
                    let timeout = self.timeout;
                    self.spawn_job(
                        path,
                        SizeSource::Manual,
                        emitter.clone(),
                        Box::new(move |job_path, cancel| {
                            match manual::calculate(Path::new(&job_path), &cancel, timeout) {
                                Ok(size_bytes) => Some(SizeUpdate {
                                    path: job_path,
                                    state: SizeStateKind::Ready,
                                    source: SizeSource::Manual,
                                    size_bytes: Some(size_bytes),
                                }),
                                Err(manual::ManualSizeError::Timeout) => Some(SizeUpdate {
                                    path: job_path,
                                    state: SizeStateKind::Na,
                                    source: SizeSource::Manual,
                                    size_bytes: None,
                                }),
                                Err(manual::ManualSizeError::Cancelled) => None,
                                Err(_) => Some(SizeUpdate {
                                    path: job_path,
                                    state: SizeStateKind::Error,
                                    source: SizeSource::Manual,
                                    size_bytes: None,
                                }),
                            }
                        }),
                    );
                }
            }
        }

        if !network_updates.is_empty() {
            emitter(network_updates);
        }

        if !everything_paths.is_empty() {
            self.spawn_everything_batch(everything_paths, emitter);
        }
    }

    fn spawn_everything_batch(
        &self,
        paths: Vec<String>,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
    ) {
        let handle = {
            let inner = self.inner.lock().expect("size service lock");
            inner.everything.clone()
        };
        let Some(handle) = handle else {
            let updates = paths
                .into_iter()
                .map(|path| SizeUpdate {
                    path,
                    state: SizeStateKind::Error,
                    source: SizeSource::Everything,
                    size_bytes: None,
                })
                .collect::<Vec<_>>();
            if !updates.is_empty() {
                emitter(updates);
            }
            return;
        };

        let jobs = paths
            .into_iter()
            .map(|path| {
                let cancel = self.register_job(&path);
                (path, cancel)
            })
            .collect::<Vec<_>>();
        let inner = Arc::clone(&self.inner);

        thread::spawn(move || {
            let requestable = jobs
                .iter()
                .filter_map(|(path, cancel)| {
                    (!cancel.load(Ordering::Relaxed)).then_some(path.clone())
                })
                .collect::<Vec<_>>();

            if requestable.is_empty() {
                for (path, cancel) in jobs {
                    SizeService::complete_job(&inner, &path, &cancel);
                }
                return;
            }

            // The whole batch is abandoned once every one of its jobs has been
            // cancelled (navigating away cancels them all at once). `.all`
            // short-circuits on the first still-active job, so this is O(1) in
            // the common case and only scans fully when actually cancelled.
            // Scoped in a block so the borrow of `jobs` ends before the result
            // loop below consumes it.
            let results = {
                let batch_cancelled = || {
                    jobs.iter()
                        .all(|(_, cancel)| cancel.load(Ordering::Relaxed))
                };

                // Announce the jobs we're actually about to query (cancelled
                // ones are skipped above), bailing out of the flood the instant
                // the batch is cancelled instead of emitting for every path.
                let mut calculating_updates = Vec::with_capacity(requestable.len());
                for path in &requestable {
                    if batch_cancelled() {
                        break;
                    }
                    calculating_updates.push(SizeUpdate {
                        path: path.clone(),
                        state: SizeStateKind::Calculating,
                        source: SizeSource::Everything,
                        size_bytes: None,
                    });
                }
                if !calculating_updates.is_empty() {
                    emitter(calculating_updates);
                }

                handle.query_folder_sizes(&requestable, &batch_cancelled)
            };

            match results {
                Ok(results) => {
                    let mut terminal_updates = Vec::new();
                    for (path, cancel) in jobs {
                        if cancel.load(Ordering::Relaxed) {
                            SizeService::complete_job(&inner, &path, &cancel);
                            continue;
                        }

                        terminal_updates.push(SizeUpdate {
                            path: path.clone(),
                            state: results
                                .get(&path)
                                .copied()
                                .flatten()
                                .map(|_| SizeStateKind::Ready)
                                .unwrap_or(SizeStateKind::Na),
                            source: SizeSource::Everything,
                            size_bytes: results.get(&path).copied().flatten(),
                        });
                        SizeService::complete_job(&inner, &path, &cancel);
                    }
                    if !terminal_updates.is_empty() {
                        emitter(terminal_updates);
                    }
                }
                Err(_) => {
                    let mut terminal_updates = Vec::new();
                    for (path, cancel) in jobs {
                        if cancel.load(Ordering::Relaxed) {
                            SizeService::complete_job(&inner, &path, &cancel);
                            continue;
                        }

                        terminal_updates.push(SizeUpdate {
                            path: path.clone(),
                            state: SizeStateKind::Error,
                            source: SizeSource::Everything,
                            size_bytes: None,
                        });
                        SizeService::complete_job(&inner, &path, &cancel);
                    }
                    if !terminal_updates.is_empty() {
                        emitter(terminal_updates);
                    }
                }
            }
        });
    }

    /// Registers a cancellation handle for `path`, superseding any prior job
    /// for the same path. Deliberately cheap and side-effect free: it does
    /// **not** emit a `Calculating` event, so enqueuing thousands of jobs is a
    /// tight, fast loop rather than thousands of synchronous IPC posts. The
    /// `Calculating` event is emitted lazily by the worker once it actually
    /// starts the job (see [`SizeService::spawn_job`] and
    /// [`SizeService::spawn_everything_batch`]), which means a job cancelled
    /// before a worker reaches it produces no events and does no work at all.
    fn register_job(&self, path: &str) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut inner = self.inner.lock().expect("size service lock");
        if let Some(previous) = inner.jobs.insert(path.to_string(), Arc::clone(&cancel)) {
            previous.store(true, Ordering::Relaxed);
        }
        cancel
    }

    fn complete_job(inner: &Arc<Mutex<SizeServiceInner>>, path: &str, cancel: &Arc<AtomicBool>) {
        let mut locked = inner.lock().expect("size service lock");
        if locked
            .jobs
            .get(path)
            .is_some_and(|registered| Arc::ptr_eq(registered, cancel))
        {
            locked.jobs.remove(path);
        }
    }

    fn spawn_job(
        &self,
        path: String,
        source: SizeSource,
        emitter: Arc<dyn Fn(Vec<SizeUpdate>) + Send + Sync>,
        work: Box<dyn FnOnce(String, Arc<AtomicBool>) -> Option<SizeUpdate> + Send + 'static>,
    ) {
        let cancel = self.register_job(&path);
        let inner = Arc::clone(&self.inner);
        let cleanup_path = path.clone();
        let cleanup_cancel = Arc::clone(&cancel);

        let task = move || {
            // A job may sit queued behind thousands of others; if it was
            // cancelled before a worker reached it (e.g. the pane navigated
            // away from a folder with tens of thousands of subfolders), skip
            // the filesystem work entirely instead of `stat`ing and walking a
            // folder whose result will be discarded. The `Calculating` event
            // is emitted here — only once work actually begins — so a cancelled
            // backlog stays completely silent.
            if !cancel.load(Ordering::Relaxed) {
                emitter(vec![SizeUpdate {
                    path: path.clone(),
                    state: SizeStateKind::Calculating,
                    source,
                    size_bytes: None,
                }]);
                if let Some(update) = work(path, cancel) {
                    emitter(vec![update]);
                }
            }
            SizeService::complete_job(&inner, &cleanup_path, &cleanup_cancel);
        };

        // Fall back to a dedicated thread only if the pool failed to build, so
        // sizing still works (unbounded) rather than silently doing nothing.
        match size_pool() {
            Some(pool) => pool.spawn(task),
            None => {
                thread::spawn(task);
            }
        }
    }
}

pub fn size_path_from_string(path: &str) -> PathBuf {
    PathBuf::from(path)
}
