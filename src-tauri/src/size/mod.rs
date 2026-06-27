pub mod everything;
pub mod manual;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::volumes::{self, VolumeInfo};

use everything::{EverythingAvailability, EverythingHandle};

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
        let availability = match inner.everything.as_ref() {
            Some(handle) => Some(handle.availability()),
            None => None,
        };

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
        F: Fn(SizeUpdate) + Send + Sync + 'static,
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

    pub fn request_path_with_volumes(
        &self,
        path: String,
        volumes: Vec<VolumeInfo>,
        emitter: Arc<dyn Fn(SizeUpdate) + Send + Sync>,
    ) {
        self.request_paths_with_volumes(vec![path], volumes, emitter);
    }

    pub fn request_paths_with_volumes(
        &self,
        paths: Vec<String>,
        volumes: Vec<VolumeInfo>,
        emitter: Arc<dyn Fn(SizeUpdate) + Send + Sync>,
    ) {
        let mut everything_paths = Vec::new();

        for path in paths {
            let backend = self.choose_backend_for_path(Path::new(&path), &volumes);
            log::debug!("size: backend {:?} chosen for {}", backend, path);

            match backend {
                SizeBackend::NetworkNa => {
                    emitter(SizeUpdate {
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

        if !everything_paths.is_empty() {
            self.spawn_everything_batch(everything_paths, emitter);
        }
    }

    fn spawn_everything_batch(
        &self,
        paths: Vec<String>,
        emitter: Arc<dyn Fn(SizeUpdate) + Send + Sync>,
    ) {
        let handle = {
            let inner = self.inner.lock().expect("size service lock");
            inner.everything.clone()
        };
        let Some(handle) = handle else {
            for path in paths {
                emitter(SizeUpdate {
                    path,
                    state: SizeStateKind::Error,
                    source: SizeSource::Everything,
                    size_bytes: None,
                });
            }
            return;
        };

        let jobs = paths
            .into_iter()
            .map(|path| {
                let cancel = self.begin_job(&path, SizeSource::Everything, &emitter);
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

            match handle.query_folder_sizes(&requestable) {
                Ok(results) => {
                    for (path, cancel) in jobs {
                        if cancel.load(Ordering::Relaxed) {
                            SizeService::complete_job(&inner, &path, &cancel);
                            continue;
                        }

                        emitter(SizeUpdate {
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
                }
                Err(_) => {
                    for (path, cancel) in jobs {
                        if cancel.load(Ordering::Relaxed) {
                            SizeService::complete_job(&inner, &path, &cancel);
                            continue;
                        }

                        emitter(SizeUpdate {
                            path: path.clone(),
                            state: SizeStateKind::Error,
                            source: SizeSource::Everything,
                            size_bytes: None,
                        });
                        SizeService::complete_job(&inner, &path, &cancel);
                    }
                }
            }
        });
    }

    fn begin_job(
        &self,
        path: &str,
        source: SizeSource,
        emitter: &Arc<dyn Fn(SizeUpdate) + Send + Sync>,
    ) -> Arc<AtomicBool> {
        self.cancel(path);

        emitter(SizeUpdate {
            path: path.to_string(),
            state: SizeStateKind::Unknown,
            source,
            size_bytes: None,
        });
        emitter(SizeUpdate {
            path: path.to_string(),
            state: SizeStateKind::Calculating,
            source,
            size_bytes: None,
        });

        let cancel = Arc::new(AtomicBool::new(false));
        self.inner
            .lock()
            .expect("size service lock")
            .jobs
            .insert(path.to_string(), cancel.clone());
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
        emitter: Arc<dyn Fn(SizeUpdate) + Send + Sync>,
        work: Box<dyn FnOnce(String, Arc<AtomicBool>) -> Option<SizeUpdate> + Send + 'static>,
    ) {
        let cancel = self.begin_job(&path, source, &emitter);
        let inner = Arc::clone(&self.inner);
        let cleanup_path = path.clone();
        let cleanup_cancel = Arc::clone(&cancel);

        thread::spawn(move || {
            if let Some(update) = work(path, cancel) {
                emitter(update);
            }
            SizeService::complete_job(&inner, &cleanup_path, &cleanup_cancel);
        });
    }
}

pub fn size_path_from_string(path: &str) -> PathBuf {
    PathBuf::from(path)
}
