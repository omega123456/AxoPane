pub mod model;
pub mod paging;
pub mod view;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::fs::{self, DirectoryEntry, FsError};
use crate::resource_coordinator::{JobClass, JobHandle, JobSpec, ResourceCoordinator};
use crate::watch::patch::{classify_patch, ClassifyPatchInput, MetadataDelta, SessionPatch};
use crate::watch::{self, CaptureHandle};

pub use model::{
    BeginNavigationRequest, BeginNavigationResponse, GetSessionRangeRequest, NavigationRevision,
    PaneId, ReleaseSessionRequest, ReleaseSessionResponse, ReviseSessionViewRequest,
    SessionBaseline, SessionId, SessionRangePage, SessionRangeResponse, SessionRejection,
    ViewParams, ViewRevision, WatchRevision, SESSION_PAGE_SIZE,
};
use view::SessionView;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaneTabKey {
    pub pane_id: PaneId,
    pub tab_id: String,
}

pub struct EnumerationAdmission<'admission> {
    pub coordinator: Option<&'admission ResourceCoordinator>,
    pub resource_key: Option<String>,
}

impl EnumerationAdmission<'_> {
    pub fn ungated() -> Self {
        Self {
            coordinator: None,
            resource_key: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct PanePath<'target> {
    pub pane_id: &'target str,
    pub path: &'target str,
}

pub struct WatchMutation {
    pub changed_entries: Vec<DirectoryEntry>,
    pub removed_paths: Vec<String>,
}

pub struct PaneSnapshot {
    pub entries: Vec<DirectoryEntry>,
    pub generation: u64,
    pub resolved_path: String,
}

#[derive(Clone, Copy)]
pub struct EnumerationTarget<'target> {
    pub path: &'target Path,
    pub view: &'target ViewParams,
}

pub struct Reconciled {
    pub snapshot: Vec<DirectoryEntry>,
    pub watch_revision: WatchRevision,
}

struct ActiveSession {
    key: PaneTabKey,
    path: String,
    session_id: SessionId,
    navigation_revision: NavigationRevision,
    watch_revision: WatchRevision,
    view_revision: ViewRevision,
    snapshot: Vec<DirectoryEntry>,
    view: SessionView,
    view_params: ViewParams,
}

impl ActiveSession {
    fn baseline(&self) -> SessionBaseline {
        SessionBaseline {
            session_id: self.session_id,
            navigation_revision: self.navigation_revision,
            watch_revision: self.watch_revision,
            view_revision: self.view_revision,
        }
    }
}

#[derive(Default)]
pub struct DirectorySessionService {
    sessions: Mutex<HashMap<PaneId, ActiveSession>>,
    navigation_reservations: Mutex<HashMap<PaneId, NavigationRevision>>,
    next_session_id: AtomicU64,
}

impl DirectorySessionService {
    pub fn begin_navigation(
        &self,
        request: BeginNavigationRequest,
        admission: EnumerationAdmission<'_>,
    ) -> Result<BeginNavigationResponse, String> {
        let navigation_revision = self.retire_pane_session(&request.pane_id);
        let session_id = SessionId(navigation_revision.0);

        let path = PathBuf::from(&request.path);

        let _permit = admit_enumeration(admission)?;

        let capture = watch::begin_capture(&path).ok();

        let entries = enumerate_snapshot(&path, &request.view)
            .map_err(|error| format!("Failed to load \"{}\": {error}", request.path))?;

        let target = EnumerationTarget {
            path: &path,
            view: &request.view,
        };
        let Reconciled {
            snapshot,
            watch_revision,
        } = reconcile_capture(target, entries, capture)?;

        let view = SessionView::derive(&snapshot, &request.view);
        let total_rows = view.total_rows();
        let first_page = view
            .page(0, SESSION_PAGE_SIZE)
            .map(|slice| slice.to_vec())
            .unwrap_or_default();

        let baseline = SessionBaseline {
            session_id,
            navigation_revision,
            watch_revision,
            view_revision: ViewRevision(0),
        };

        let key = PaneTabKey {
            pane_id: request.pane_id.clone(),
            tab_id: request.tab_id.clone(),
        };
        let resolved_path = display_path(&path);

        let session = ActiveSession {
            key,
            path: resolved_path.clone(),
            session_id,
            navigation_revision,
            watch_revision,
            view_revision: ViewRevision(0),
            snapshot,
            view,
            view_params: request.view,
        };

        self.publish_session(session)?;

        Ok(BeginNavigationResponse {
            pane_id: request.pane_id,
            tab_id: request.tab_id,
            path: resolved_path,
            baseline,
            total_rows,
            page_size: SESSION_PAGE_SIZE as u64,
            first_page: SessionRangePage {
                page_index: 0,
                entries: first_page,
            },
        })
    }

    fn retire_pane_session(&self, pane_id: &str) -> NavigationRevision {
        let navigation_revision =
            NavigationRevision(self.next_session_id.fetch_add(1, Ordering::SeqCst));
        let mut reservations = self
            .navigation_reservations
            .lock()
            .expect("directory navigation reservation lock");
        reservations.insert(pane_id.to_string(), navigation_revision);
        self.sessions
            .lock()
            .expect("directory session lock")
            .remove(pane_id);
        navigation_revision
    }

    fn publish_session(&self, session: ActiveSession) -> Result<(), String> {
        let mut reservations = self
            .navigation_reservations
            .lock()
            .expect("directory navigation reservation lock");
        let mut sessions = self.sessions.lock().expect("directory session lock");
        let pane_id = session.key.pane_id.clone();
        let still_reserved = reservations.get(&pane_id) == Some(&session.navigation_revision);
        if !still_reserved {
            return Err("Directory navigation was superseded.".to_string());
        }
        reservations.remove(&pane_id);
        sessions.insert(pane_id, session);
        Ok(())
    }

    pub fn revise_view(
        &self,
        request: ReviseSessionViewRequest,
    ) -> Result<BeginNavigationResponse, SessionRejection> {
        let ReviseSessionViewRequest {
            pane_id,
            tab_id,
            session_id,
            view,
        } = request;
        let mut sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions
            .get_mut(&pane_id)
            .ok_or(SessionRejection::NoActiveSession)?;

        if session.key.tab_id != tab_id {
            return Err(SessionRejection::NoActiveSession);
        }
        if session.session_id != session_id {
            return Err(SessionRejection::StaleSession);
        }

        session.view_revision = ViewRevision(session.view_revision.0 + 1);
        session.view = SessionView::derive(&session.snapshot, &view);
        session.view_params = view;

        let total_rows = session.view.total_rows();
        let first_page = session
            .view
            .page(0, SESSION_PAGE_SIZE)
            .map(|slice| slice.to_vec())
            .unwrap_or_default();

        Ok(BeginNavigationResponse {
            pane_id,
            tab_id: session.key.tab_id.clone(),
            path: session.path.clone(),
            baseline: session.baseline(),
            total_rows,
            page_size: SESSION_PAGE_SIZE as u64,
            first_page: SessionRangePage {
                page_index: 0,
                entries: first_page,
            },
        })
    }

    pub fn get_range(
        &self,
        request: &GetSessionRangeRequest,
    ) -> Result<SessionRangeResponse, SessionRejection> {
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions
            .get(&request.pane_id)
            .ok_or(SessionRejection::NoActiveSession)?;

        let expected = paging::ExpectedSession {
            key: &session.key,
            baseline: session.baseline(),
            view: &session.view,
        };
        paging::materialize_range(request, expected)
    }

    pub fn release_session(&self, request: &ReleaseSessionRequest) -> ReleaseSessionResponse {
        let mut reservations = self
            .navigation_reservations
            .lock()
            .expect("directory navigation reservation lock");
        let mut sessions = self.sessions.lock().expect("directory session lock");
        let Some(session) = sessions.get(&request.pane_id) else {
            return ReleaseSessionResponse { released: false };
        };

        let matches = session.key.tab_id == request.tab_id
            && session.session_id == request.session_id
            && session.navigation_revision == request.navigation_revision;

        if !matches {
            return ReleaseSessionResponse { released: false };
        }

        sessions.remove(&request.pane_id);
        reservations.remove(&request.pane_id);
        ReleaseSessionResponse { released: true }
    }

    pub fn active_session_count(&self) -> usize {
        self.sessions.lock().expect("directory session lock").len()
    }

    pub fn current_baseline(&self, pane_id: &PaneId) -> Option<SessionBaseline> {
        self.sessions
            .lock()
            .expect("directory session lock")
            .get(pane_id)
            .map(ActiveSession::baseline)
    }

    pub fn snapshot_for_pane_path(&self, pane_id: &PaneId, path: &str) -> Option<PaneSnapshot> {
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get(pane_id)?;
        if !fs_path_matches(&session.path, path) {
            return None;
        }
        Some(PaneSnapshot {
            entries: session.snapshot.clone(),
            generation: session.watch_revision.0,
            resolved_path: session.path.clone(),
        })
    }

    pub fn watch_revision_for_pane_path(&self, pane_id: &PaneId, path: &str) -> Option<u64> {
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get(pane_id)?;
        fs_path_matches(&session.path, path).then_some(session.watch_revision.0)
    }

    pub fn apply_watch_mutation(
        &self,
        target: PanePath<'_>,
        mutation: WatchMutation,
    ) -> Option<SessionPatch> {
        let PanePath { pane_id, path } = target;
        let WatchMutation {
            changed_entries,
            removed_paths,
        } = mutation;
        let mut sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get_mut(pane_id)?;
        if !fs_path_matches(&session.path, path) {
            return None;
        }

        let previous_baseline = session.baseline();
        let previous_view_rows = session.view.rows().to_vec();

        let removed_set: std::collections::HashSet<&str> =
            removed_paths.iter().map(String::as_str).collect();
        session
            .snapshot
            .retain(|entry| !removed_set.contains(entry.path.as_str()));
        for changed in changed_entries {
            upsert_entry_by_path(&mut session.snapshot, changed);
        }

        session.view = SessionView::derive(&session.snapshot, &session.view_params);
        let next_view_rows = session.view.rows().to_vec();

        if previous_view_rows == next_view_rows {
            return None;
        }

        session.watch_revision = WatchRevision(session.watch_revision.0 + 1);
        session.view_revision = ViewRevision(session.view_revision.0 + 1);
        let next_baseline = session.baseline();

        let changed_paths: Vec<String> = next_view_rows
            .iter()
            .filter(|entry| {
                !previous_view_rows
                    .iter()
                    .any(|previous| previous.path == entry.path && previous == *entry)
            })
            .map(|entry| entry.path.clone())
            .collect();
        let removed_view_paths: Vec<String> = previous_view_rows
            .iter()
            .filter(|entry| !next_view_rows.iter().any(|next| next.path == entry.path))
            .map(|entry| entry.path.clone())
            .collect();

        classify_patch(ClassifyPatchInput {
            pane_id,
            tab_id: &session.key.tab_id,
            path: &session.path,
            previous_baseline,
            next_baseline,
            previous_view: &previous_view_rows,
            next_view: &next_view_rows,
            changed_paths: &changed_paths,
            removed_paths: &removed_view_paths,
        })
    }

    pub fn apply_metadata_only(
        &self,
        target: PanePath<'_>,
        updates: Vec<MetadataDelta>,
    ) -> Option<SessionPatch> {
        let PanePath { pane_id, path } = target;
        if updates.is_empty() {
            return None;
        }
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get(pane_id)?;
        if !fs_path_matches(&session.path, path) {
            return None;
        }

        Some(SessionPatch::MetadataOnly {
            pane_id: pane_id.to_string(),
            tab_id: session.key.tab_id.clone(),
            path: session.path.clone(),
            baseline: session.baseline(),
            updates,
        })
    }

    pub fn resnapshot(&self, pane_id: &PaneId) -> Option<SessionPatch> {
        let mut sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get_mut(pane_id)?;
        let path = PathBuf::from(&session.path);
        let entries = enumerate_snapshot(&path, &session.view_params).ok()?;

        let previous_baseline = session.baseline();
        session.snapshot = entries;
        session.view = SessionView::derive(&session.snapshot, &session.view_params);
        session.watch_revision = WatchRevision(session.watch_revision.0 + 1);
        session.view_revision = ViewRevision(session.view_revision.0 + 1);
        let next_baseline = session.baseline();

        Some(SessionPatch::ReplaceView {
            pane_id: pane_id.clone(),
            tab_id: session.key.tab_id.clone(),
            path: session.path.clone(),
            previous_baseline,
            next_baseline,
            total_rows: session.view.total_rows(),
        })
    }
}

fn fs_path_matches(session_path: &str, candidate: &str) -> bool {
    if session_path == candidate {
        return true;
    }
    let session_normalized = fs::canonicalize_dir(Path::new(session_path))
        .map(|path| fs::display_path_from_path(&path))
        .unwrap_or_else(|_| fs::display_path_from_path(Path::new(session_path)));
    let candidate_normalized = fs::canonicalize_dir(Path::new(candidate))
        .map(|path| fs::display_path_from_path(&path))
        .unwrap_or_else(|_| fs::display_path_from_path(Path::new(candidate)));
    session_normalized == candidate_normalized
        || session_normalized.eq_ignore_ascii_case(&candidate_normalized)
}

fn enumerate_snapshot(path: &Path, view: &ViewParams) -> Result<Vec<DirectoryEntry>, FsError> {
    let options = fs::ListDirOptions {
        path: path.to_string_lossy().into_owned(),
        sort_key: view.sort_key,
        sort_direction: view.sort_direction,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: view.include_item_counts,
    };
    fs::list_dir(&options).map(|response| response.entries)
}

fn reconcile_capture(
    target: EnumerationTarget<'_>,
    entries: Vec<DirectoryEntry>,
    capture: Option<CaptureHandle>,
) -> Result<Reconciled, String> {
    let Some(capture) = capture else {
        return Ok(Reconciled::initial(entries));
    };

    resolve_reconciliation(target, entries, watch::drain_captured_mutations(capture))
}

impl Reconciled {
    fn initial(snapshot: Vec<DirectoryEntry>) -> Self {
        Self {
            snapshot,
            watch_revision: WatchRevision(0),
        }
    }

    fn resnapshotted(snapshot: Vec<DirectoryEntry>) -> Self {
        Self {
            snapshot,
            watch_revision: WatchRevision(1),
        }
    }
}

fn resolve_reconciliation(
    target: EnumerationTarget<'_>,
    entries: Vec<DirectoryEntry>,
    drained: Option<std::collections::HashSet<PathBuf>>,
) -> Result<Reconciled, String> {
    let EnumerationTarget { path, view } = target;
    match drained {
        Some(mutated) if mutated.is_empty() => Ok(Reconciled::initial(entries)),
        Some(_) | None => {
            let resnapshot = enumerate_snapshot(path, view)
                .map_err(|error| format!("Failed to resnapshot \"{}\": {error}", path.display()))?;
            Ok(Reconciled::resnapshotted(resnapshot))
        }
    }
}

fn display_path(path: &Path) -> String {
    fs::canonicalize_dir(path)
        .map(|canonical| fs::display_path_from_path(&canonical))
        .unwrap_or_else(|_| fs::display_path_from_path(path))
}

#[cfg(feature = "test-utils")]
pub fn enumerate_snapshot_for_tests(
    path: &Path,
    view: &ViewParams,
) -> Result<Vec<DirectoryEntry>, FsError> {
    enumerate_snapshot(path, view)
}

#[cfg(feature = "test-utils")]
pub fn resolve_reconciliation_for_tests(
    target: EnumerationTarget<'_>,
    entries: Vec<DirectoryEntry>,
    drained: Option<std::collections::HashSet<PathBuf>>,
) -> Result<Reconciled, String> {
    resolve_reconciliation(target, entries, drained)
}

fn admit_enumeration(admission: EnumerationAdmission<'_>) -> Result<Option<JobHandle>, String> {
    let EnumerationAdmission {
        coordinator,
        resource_key,
    } = admission;
    let (Some(coordinator), Some(resource_key)) = (coordinator, resource_key) else {
        return Ok(None);
    };
    let spec = JobSpec::new([JobClass::Latency], [resource_key]);
    coordinator
        .submit(spec)
        .map(Some)
        .map_err(|error| format!("resource admission failed: {error:?}"))
}

fn upsert_entry_by_path(snapshot: &mut Vec<DirectoryEntry>, changed: DirectoryEntry) {
    let existing = snapshot.iter_mut().find(|entry| entry.path == changed.path);
    if let Some(existing) = existing {
        *existing = changed;
        return;
    }
    snapshot.push(changed);
}
