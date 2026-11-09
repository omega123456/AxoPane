#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::manual::{self, ManualSizeError};
use super::worker_pool::{ManualJobRequest, ManualWork};
use super::{
    emit_non_empty, NaReason, RequestMode, SizeBackend, SizeRequest, SizeRequestWithVolumes,
    SizeService, SizeSource, SizeStateKind, SizeUpdate, UpdateEmitter,
};
#[cfg(target_os = "macos")]
use crate::volumes;
use crate::volumes::VolumeInfo;

#[cfg(target_os = "macos")]
pub(crate) enum Route {
    AsIs,
    Target(PathBuf),
    NotDir,
}

#[cfg(target_os = "macos")]
pub(crate) fn route_of(path: &Path) -> Route {
    match std::fs::canonicalize(path) {
        Ok(target) if !target.is_dir() => Route::NotDir,
        Ok(target) if target == path => Route::AsIs,
        Ok(target) => Route::Target(target),
        Err(_) if path.is_symlink() => Route::NotDir,
        Err(_) => Route::AsIs,
    }
}

#[cfg(target_os = "macos")]
fn request_route(requested: &Path, volumes: &[VolumeInfo]) -> Route {
    if volumes::path_is_network(requested, volumes) {
        return Route::AsIs;
    }
    route_of(requested)
}

struct RequestTarget {
    sized: PathBuf,
    #[cfg(target_os = "macos")]
    route: Route,
}

impl RequestTarget {
    #[cfg(target_os = "macos")]
    fn resolve(path: &str, volumes: &[VolumeInfo]) -> Self {
        let requested = Path::new(path);
        let route = request_route(requested, volumes);
        let sized = match &route {
            Route::Target(target) => target.clone(),
            Route::AsIs | Route::NotDir => PathBuf::from(path),
        };
        Self { sized, route }
    }

    #[cfg(not(target_os = "macos"))]
    fn resolve(path: &str, _volumes: &[VolumeInfo]) -> Self {
        Self {
            sized: PathBuf::from(path),
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct IndexRequests {
    plain: Vec<String>,
    linked: Vec<(String, PathBuf)>,
}

#[cfg(target_os = "macos")]
impl IndexRequests {
    fn push(&mut self, path: String, route: Route) {
        match route {
            Route::Target(target) => self.linked.push((path, target)),
            Route::AsIs | Route::NotDir => self.plain.push(path),
        }
    }
}

#[derive(Clone, Copy)]
struct Delivery<'delivery> {
    mode: RequestMode,
    emitter: &'delivery UpdateEmitter,
}

struct RoutingPass<'pass> {
    delivery: Delivery<'pass>,
    volumes: &'pass [VolumeInfo],
    routed: RoutedRequests,
}

#[derive(Default)]
struct RoutedRequests {
    everything: Vec<String>,
    immediate: Vec<SizeUpdate>,
    #[cfg(target_os = "macos")]
    index: IndexRequests,
}

impl SizeService {
    pub fn request_paths_with_mode(
        &self,
        routed_request: SizeRequestWithVolumes,
        emitter: UpdateEmitter,
    ) {
        let SizeRequestWithVolumes {
            request: SizeRequest { paths, mode },
            volumes,
        } = routed_request;
        let delivery = Delivery {
            mode,
            emitter: &emitter,
        };
        let mut pass = RoutingPass {
            delivery,
            volumes: &volumes,
            routed: RoutedRequests::default(),
        };
        for path in paths {
            self.route_request(&mut pass, path);
        }

        let routed = pass.routed;
        emit_non_empty(&emitter, routed.immediate);
        #[cfg(target_os = "macos")]
        self.request_from_index(routed.index, delivery);
        let everything_paths = routed.everything;
        if !everything_paths.is_empty() {
            self.spawn_everything_batch(everything_paths, emitter);
        }
    }

    fn route_request(&self, pass: &mut RoutingPass<'_>, path: String) {
        let request = RequestTarget::resolve(&path, pass.volumes);
        let mode = pass.delivery.mode;
        #[cfg(not(target_os = "macos"))]
        let _ = mode;
        let routed = &mut pass.routed;
        match self.backend_for(&request, pass.volumes) {
            #[cfg(target_os = "macos")]
            SizeBackend::Index => routed.index.push(path, request.route),
            #[cfg(target_os = "macos")]
            SizeBackend::Manual if mode == RequestMode::Auto => {
                routed.immediate.push(SizeUpdate::without_size(
                    path,
                    SizeStateKind::Unknown,
                    SizeSource::Manual,
                ));
            }
            SizeBackend::NetworkNa => {
                routed.immediate.push(SizeUpdate::na(
                    path,
                    SizeSource::Network,
                    Some(NaReason::Network),
                ));
            }
            SizeBackend::Everything => routed.everything.push(path),
            SizeBackend::Manual => self.enqueue_manual_job(ManualJobRequest {
                path,
                source: SizeSource::Manual,
                emitter: Arc::clone(pass.delivery.emitter),
                work: manual_size_work(request.sized, self.timeout),
            }),
        }
    }

    fn backend_for(&self, request: &RequestTarget, volumes: &[VolumeInfo]) -> SizeBackend {
        let backend = self.choose_backend_for_path(&request.sized, volumes);
        #[cfg(target_os = "macos")]
        if is_indexed_non_directory(backend, request) {
            return SizeBackend::Manual;
        }
        backend
    }

    #[cfg(target_os = "macos")]
    fn request_from_index(&self, requests: IndexRequests, delivery: Delivery<'_>) {
        let Some(index) = self.folder_size_index() else {
            return;
        };
        let Delivery { mode, emitter } = delivery;
        let plain_paths = requests.plain;
        if !plain_paths.is_empty() {
            let plain = SizeRequest {
                paths: plain_paths,
                mode,
            };
            index.request(plain, Arc::clone(emitter));
        }
        for (path, target) in requests.linked {
            let linked = SizeRequest {
                paths: vec![target.to_string_lossy().into_owned()],
                mode,
            };
            index.request(linked, reported_as(path, Arc::clone(emitter)));
        }
    }
}

#[cfg(target_os = "macos")]
fn reported_as(path: String, emitter: UpdateEmitter) -> UpdateEmitter {
    Arc::new(move |updates: Vec<SizeUpdate>| {
        let renamed = updates
            .into_iter()
            .map(|update| SizeUpdate {
                path: path.clone(),
                ..update
            })
            .collect();
        emitter(renamed);
    })
}

fn manual_size_work(sized: PathBuf, timeout: Duration) -> ManualWork {
    Box::new(move |job_path, cancel| {
        manual_size_update(job_path, manual::calculate(&sized, &cancel, timeout))
    })
}

fn manual_size_update(
    job_path: String,
    result: Result<u64, ManualSizeError>,
) -> Option<SizeUpdate> {
    match result {
        Ok(size_bytes) => Some(SizeUpdate::ready(job_path, SizeSource::Manual, size_bytes)),
        Err(ManualSizeError::Timeout) => Some(SizeUpdate::na(
            job_path,
            SizeSource::Manual,
            Some(NaReason::Timeout),
        )),
        Err(ManualSizeError::Cancelled) => None,
        Err(_) => Some(SizeUpdate::without_size(
            job_path,
            SizeStateKind::Error,
            SizeSource::Manual,
        )),
    }
}

#[cfg(target_os = "macos")]
fn is_indexed_non_directory(backend: SizeBackend, request: &RequestTarget) -> bool {
    backend == SizeBackend::Index && matches!(request.route, Route::NotDir)
}
