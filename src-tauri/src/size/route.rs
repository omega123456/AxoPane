#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::manual::{self, ManualSizeError};
use super::worker_pool::ManualWork;
use super::{
    emit_non_empty, NaReason, RequestMode, SizeBackend, SizeService, SizeSource, SizeStateKind,
    SizeUpdate, UpdateEmitter,
};
#[cfg(target_os = "macos")]
use crate::volumes;
use crate::volumes::VolumeInfo;

/// How a macOS size request is routed, resolved once at request time.
#[cfg(target_os = "macos")]
pub(crate) enum Route {
    /// By the requested path itself: already canonical, or missing.
    AsIs,
    /// By this canonical directory: the requested path runs through a
    /// symlink (walks themselves never follow one).
    Target(PathBuf),
    /// Not a directory: a file, a link to one, or a dangling or looping link.
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

/// The path a request is sized by, plus (on macOS) how it was routed.
struct RequestTarget {
    sized: PathBuf,
    #[cfg(target_os = "macos")]
    route: Route,
}

impl RequestTarget {
    /// Network paths stay `na` without touching the share.
    #[cfg(target_os = "macos")]
    fn resolve(path: &str, volumes: &[VolumeInfo]) -> Self {
        let requested = Path::new(path);
        let route = if volumes::path_is_network(requested, volumes) {
            Route::AsIs
        } else {
            route_of(requested)
        };
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
    /// Requested paths with the symlink target their total lives at.
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

#[derive(Default)]
struct RoutedRequests {
    everything: Vec<String>,
    /// Answered at once without any work.
    immediate: Vec<SizeUpdate>,
    #[cfg(target_os = "macos")]
    index: IndexRequests,
}

impl SizeService {
    /// Routes each path: network → Everything → index → manual. On macOS an
    /// `auto` request outside the index is answered `unknown` without work,
    /// and a path through a symlink is routed (and sized) by its resolved
    /// target while its updates keep the requested path.
    pub fn request_paths_with_mode(
        &self,
        paths: Vec<String>,
        mode: RequestMode,
        volumes: Vec<VolumeInfo>,
        emitter: UpdateEmitter,
    ) {
        let mut routed = RoutedRequests::default();
        for path in paths {
            self.route_request(path, mode, &volumes, &emitter, &mut routed);
        }

        emit_non_empty(&emitter, routed.immediate);
        #[cfg(target_os = "macos")]
        self.request_from_index(routed.index, mode, &emitter);
        if !routed.everything.is_empty() {
            self.spawn_everything_batch(routed.everything, emitter);
        }
    }

    fn route_request(
        &self,
        path: String,
        mode: RequestMode,
        volumes: &[VolumeInfo],
        emitter: &UpdateEmitter,
        routed: &mut RoutedRequests,
    ) {
        let request = RequestTarget::resolve(&path, volumes);
        #[cfg(not(target_os = "macos"))]
        let _ = mode;
        match self.backend_for(&request, volumes) {
            #[cfg(target_os = "macos")]
            SizeBackend::Index => routed.index.push(path, request.route),
            #[cfg(target_os = "macos")]
            SizeBackend::Manual if mode == RequestMode::Auto => {
                routed.immediate.push(SizeUpdate::new(
                    path,
                    SizeStateKind::Unknown,
                    SizeSource::Manual,
                    None,
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
            SizeBackend::Manual => self.enqueue_manual_job(
                path,
                SizeSource::Manual,
                Arc::clone(emitter),
                manual_size_work(request.sized, self.timeout),
            ),
        }
    }

    /// Files and dangling or looping links are never index paths.
    fn backend_for(&self, request: &RequestTarget, volumes: &[VolumeInfo]) -> SizeBackend {
        let backend = self.choose_backend_for_path(&request.sized, volumes);
        #[cfg(target_os = "macos")]
        if backend == SizeBackend::Index && matches!(request.route, Route::NotDir) {
            return SizeBackend::Manual;
        }
        backend
    }

    #[cfg(target_os = "macos")]
    fn request_from_index(
        &self,
        requests: IndexRequests,
        mode: RequestMode,
        emitter: &UpdateEmitter,
    ) {
        let Some(index) = self.folder_size_index() else {
            return;
        };
        if !requests.plain.is_empty() {
            index.request(requests.plain, mode, Arc::clone(emitter));
        }
        for (path, target) in requests.linked {
            index.request(
                vec![target.to_string_lossy().into_owned()],
                mode,
                reported_as(path, Arc::clone(emitter)),
            );
        }
    }
}

/// Forwards updates to `emitter` under the requested `path`.
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
        Ok(size_bytes) => Some(SizeUpdate::new(
            job_path,
            SizeStateKind::Ready,
            SizeSource::Manual,
            Some(size_bytes),
        )),
        Err(ManualSizeError::Timeout) => Some(SizeUpdate::na(
            job_path,
            SizeSource::Manual,
            Some(NaReason::Timeout),
        )),
        Err(ManualSizeError::Cancelled) => None,
        Err(_) => Some(SizeUpdate::new(
            job_path,
            SizeStateKind::Error,
            SizeSource::Manual,
            None,
        )),
    }
}
