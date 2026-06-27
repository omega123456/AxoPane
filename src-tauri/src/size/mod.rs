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
    inner: Mutex<SizeServiceInner>,
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
            inner: Mutex::new(SizeServiceInner {
                jobs: HashMap::new(),
                everything,
            }),
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

        for path in paths {
            self.request_path_with_volumes(path, volumes.clone(), emitter.clone());
        }
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
            SizeBackend::Everything => {
                let handle = {
                    let inner = self.inner.lock().expect("size service lock");
                    inner.everything.clone()
                };
                let Some(handle) = handle else {
                    return;
                };
                self.spawn_job(
                    path,
                    SizeSource::Everything,
                    emitter,
                    Box::new(move |job_path, cancel| {
                        if cancel.load(Ordering::Relaxed) {
                            return Some(SizeUpdate {
                                path: job_path,
                                state: SizeStateKind::Error,
                                source: SizeSource::Everything,
                                size_bytes: None,
                            });
                        }

                        match handle.query_folder_size(Path::new(&job_path)) {
                            Ok(Some(size_bytes)) => Some(SizeUpdate {
                                path: job_path,
                                state: SizeStateKind::Ready,
                                source: SizeSource::Everything,
                                size_bytes: Some(size_bytes),
                            }),
                            Ok(None) => Some(SizeUpdate {
                                path: job_path,
                                state: SizeStateKind::Na,
                                source: SizeSource::Everything,
                                size_bytes: None,
                            }),
                            Err(_) => Some(SizeUpdate {
                                path: job_path,
                                state: SizeStateKind::Error,
                                source: SizeSource::Everything,
                                size_bytes: None,
                            }),
                        }
                    }),
                );
            }
            SizeBackend::Manual => {
                let timeout = self.timeout;
                self.spawn_job(
                    path,
                    SizeSource::Manual,
                    emitter,
                    Box::new(move |job_path, cancel| match manual::calculate(
                        Path::new(&job_path),
                        &cancel,
                        timeout,
                    ) {
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
                    }),
                );
            }
        }
    }

    fn spawn_job(
        &self,
        path: String,
        source: SizeSource,
        emitter: Arc<dyn Fn(SizeUpdate) + Send + Sync>,
        work: Box<dyn FnOnce(String, Arc<AtomicBool>) -> Option<SizeUpdate> + Send + 'static>,
    ) {
        self.cancel(&path);

        emitter(SizeUpdate {
            path: path.clone(),
            state: SizeStateKind::Unknown,
            source,
            size_bytes: None,
        });
        emitter(SizeUpdate {
            path: path.clone(),
            state: SizeStateKind::Calculating,
            source,
            size_bytes: None,
        });

        let cancel = Arc::new(AtomicBool::new(false));
        self.inner
            .lock()
            .expect("size service lock")
            .jobs
            .insert(path.clone(), cancel.clone());

        thread::spawn(move || {
            if let Some(update) = work(path, cancel) {
                emitter(update);
            }
        });
    }
}

pub fn size_path_from_string(path: &str) -> PathBuf {
    PathBuf::from(path)
}
