//! Directory sessions: one authoritative snapshot + derived view per
//! *active* pane, seekable idempotent page-index ranges, and explicit
//! begin-navigation / release-session lifecycle.
//!
//! # Watch-first baseline
//!
//! `begin_navigation` registers a short-lived native watch
//! ([`crate::watch::begin_capture`]) *before* enumerating the directory, so a
//! filesystem mutation that races the enumeration is never silently dropped:
//! once enumeration finishes, captured mutation paths are folded into the
//! snapshot (or, if the capture reports it cannot resolve exactly what
//! changed, the directory is resnapshotted). Only once that reconciliation is
//! done does the session publish its session/watch/view baseline — every
//! range response after that carries the same baseline until a later
//! navigation or view change advances it.
//!
//! # Pull-based ranges
//!
//! There is no push/event delivery for row data in this module: the caller
//! asks for `page_index` N via [`DirectorySessionService::get_range`], and
//! that exact request is idempotent for as long as its baseline is current.
//! Nothing here streams rows unprompted.

pub mod model;
pub mod paging;
pub mod view;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::fs::{self, DirectoryEntry, FsError};
use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};
use crate::watch::patch::{classify_patch, ClassifyPatchInput, MetadataDelta, SessionPatch};
use crate::watch::{self, CaptureHandle};

pub use model::{
    BeginNavigationRequest, BeginNavigationResponse, GetSessionRangeRequest, NavigationRevision,
    PaneId, ReleaseSessionRequest, ReleaseSessionResponse, ReviseSessionViewRequest,
    SessionBaseline, SessionId, SessionRangePage, SessionRangeResponse, SessionRejection,
    ViewParams, ViewRevision, WatchRevision, SESSION_PAGE_SIZE,
};
use view::SessionView;

/// Identifies one pane+tab so `begin_navigation`/`release_session` can be
/// scoped precisely (Phase 3 keeps at most one active session *per pane*, but
/// the tab id is still carried and validated so a release/range request can
/// never accidentally cross tabs within the same pane).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PaneTabKey {
    pane_id: PaneId,
    tab_id: String,
}

/// Everything the session owns for one active pane: the unfiltered snapshot,
/// the derived view, and the full revision set that must match on every
/// request.
struct ActiveSession {
    key: PaneTabKey,
    path: String,
    session_id: SessionId,
    navigation_revision: NavigationRevision,
    watch_revision: WatchRevision,
    view_revision: ViewRevision,
    /// Unfiltered immutable snapshot as returned by enumeration (post
    /// watch-first reconciliation). Never re-read from disk for filter/sort
    /// changes — only `view` is rebuilt.
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

/// Rust-owned service backing the directory-session IPC contract. Managed as
/// Tauri state (`DirectorySessionService::default()`) alongside
/// [`crate::watch::WatchService`].
#[derive(Default)]
pub struct DirectorySessionService {
    sessions: Mutex<HashMap<PaneId, ActiveSession>>,
    /// Reservation made before navigation I/O begins.  A slower older
    /// enumeration may finish later, but it can never publish over this
    /// newer reservation.
    navigation_reservations: Mutex<HashMap<PaneId, NavigationRevision>>,
    next_session_id: AtomicU64,
}

impl DirectorySessionService {
    /// Establishes a new session for `request.pane_id`/`request.tab_id`,
    /// retiring any prior session for that pane first (Functional
    /// Requirement 5: begin-navigation retires prior ownership before
    /// establishing the new one). Performs the watch-first handshake: begins
    /// mutation capture, enumerates, reconciles or resnapshots, then
    /// publishes one baseline.
    ///
    /// `coordinator`/`resource_key` are optional: when `resource_key` is
    /// `Some`, enumeration is gated behind a `Latency`-class coordinator
    /// admission for that resource so directory-session work participates in
    /// the same fairness/backpressure model as every other subsystem
    /// migrating onto the coordinator. When `None` (e.g. an unresolvable
    /// path), enumeration proceeds ungated — session correctness never
    /// depends on volume identity being resolvable.
    pub fn begin_navigation(
        &self,
        request: BeginNavigationRequest,
        coordinator: Option<&ResourceCoordinator>,
        resource_key: Option<String>,
    ) -> Result<BeginNavigationResponse, String> {
        // Retire any existing session for this pane up front, before doing
        // any filesystem work, so a rapid double-navigation never leaves two
        // sessions briefly alive for the same pane.
        let navigation_revision =
            NavigationRevision(self.next_session_id.fetch_add(1, Ordering::SeqCst));
        let session_id = SessionId(navigation_revision.0);
        {
            let mut reservations = self
                .navigation_reservations
                .lock()
                .expect("directory navigation reservation lock");
            reservations.insert(request.pane_id.clone(), navigation_revision);
            self.sessions
                .lock()
                .expect("directory session lock")
                .remove(&request.pane_id);
        }

        let path = PathBuf::from(&request.path);

        let _permit = match (coordinator, resource_key.as_ref()) {
            (Some(coordinator), Some(resource_key)) => {
                let spec = JobSpec::new([JobClass::Latency], [resource_key.clone()]);
                match coordinator.submit(spec) {
                    Ok(handle) => Some(handle),
                    Err(error) => return Err(format!("resource admission failed: {error:?}")),
                }
            }
            _ => None,
        };

        // Watch-first: start capturing direct-child mutations before the
        // directory is enumerated so nothing that happens during enumeration
        // can be lost.
        let capture = watch::begin_capture(&path).ok();

        let entries = enumerate_snapshot(&path, &request.view)
            .map_err(|error| format!("Failed to load \"{}\": {error}", request.path))?;

        let (snapshot, watch_revision) = reconcile_capture(&path, &request.view, entries, capture)?;

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

        let mut reservations = self
            .navigation_reservations
            .lock()
            .expect("directory navigation reservation lock");
        if reservations.get(&request.pane_id) != Some(&navigation_revision) {
            return Err("Directory navigation was superseded.".to_string());
        }
        let mut sessions = self.sessions.lock().expect("directory session lock");
        // The reservation is held while publishing so a newer navigation
        // cannot interleave an old session insert between its retirement and
        // its own reservation.
        if reservations.get(&request.pane_id) != Some(&navigation_revision) {
            return Err("Directory navigation was superseded.".to_string());
        }
        sessions.insert(request.pane_id.clone(), session);
        reservations.remove(&request.pane_id);

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

    /// Revises the view (sort/filter/show-hidden/item-counts) for the
    /// currently active session on `pane_id`, without re-enumerating the
    /// filesystem. Returns the new baseline (bumped `view_revision`, same
    /// `session_id`/`navigation_revision`/`watch_revision`) and first page,
    /// or a rejection if there is no active session / it does not match
    /// `tab_id`/`session_id`.
    pub fn revise_view(
        &self,
        pane_id: &PaneId,
        tab_id: &str,
        session_id: SessionId,
        view: ViewParams,
    ) -> Result<BeginNavigationResponse, SessionRejection> {
        let mut sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions
            .get_mut(pane_id)
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
            pane_id: pane_id.clone(),
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

    /// Idempotent page-index range fetch scoped to `request.baseline`. Any
    /// valid page index may be requested repeatedly (forward or backward) and
    /// returns identical content while the baseline stays current; a stale
    /// baseline field (session/navigation/watch/view) is rejected rather than
    /// served (Functional Requirement 1).
    pub fn get_range(
        &self,
        request: &GetSessionRangeRequest,
    ) -> Result<SessionRangeResponse, SessionRejection> {
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions
            .get(&request.pane_id)
            .ok_or(SessionRejection::NoActiveSession)?;

        paging::materialize_range(
            request,
            &session.key.pane_id,
            &session.key.tab_id,
            session.baseline(),
            &session.view,
        )
    }

    /// Idempotent release: retires the active session for `request.pane_id`
    /// only if it still matches `tab_id`/`session_id`/`navigation_revision`.
    /// Calling this twice (or during teardown, when the session may already
    /// be gone) is always safe and simply reports `released: false` the
    /// second time.
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

    /// Total number of currently active sessions. Used by tests/diagnostics
    /// to assert the "at most two active sessions total, one per pane"
    /// retention rule (Cache and Retention Policies table).
    pub fn active_session_count(&self) -> usize {
        self.sessions.lock().expect("directory session lock").len()
    }

    /// Returns the current baseline for `pane_id`, if any. Test/diagnostic
    /// convenience so callers do not need to round-trip through
    /// `begin_navigation`'s response to learn the active baseline.
    pub fn current_baseline(&self, pane_id: &PaneId) -> Option<SessionBaseline> {
        self.sessions
            .lock()
            .expect("directory session lock")
            .get(pane_id)
            .map(ActiveSession::baseline)
    }

    /// Returns the active session's unfiltered snapshot entries for
    /// `pane_id`, along with its current `watch_revision`, only if that
    /// session's path matches `path` (exact-first, `fs_path_matches`
    /// fallback — see that helper's docs). Read-only: never re-reads disk,
    /// mirroring `current_baseline`'s style.
    ///
    /// Callers that want to *derive* a sorted/filtered result without a
    /// fresh directory read (e.g. `ItemCountService::sort_active_items`) use
    /// this instead of `SessionView`'s already-derived view because the
    /// requested view parameters (sort key/direction/filter) may differ from
    /// whatever the session's own `view_params` currently holds — deriving a
    /// fresh [`view::SessionView`] from the *unfiltered* snapshot is what
    /// lets the caller apply its own requested view without stomping on the
    /// session's actual active view.
    pub fn snapshot_for_pane_path(
        &self,
        pane_id: &PaneId,
        path: &str,
    ) -> Option<(Vec<DirectoryEntry>, u64, String)> {
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get(pane_id)?;
        if !fs_path_matches(&session.path, path) {
            return None;
        }
        Some((
            session.snapshot.clone(),
            session.watch_revision.0,
            session.path.clone(),
        ))
    }

    /// Returns the mutation-driven generation for the active matching
    /// session without cloning its snapshot. Viewport item-count requests use
    /// this so their cache keys match an Items sort of the same snapshot.
    pub fn watch_revision_for_pane_path(&self, pane_id: &PaneId, path: &str) -> Option<u64> {
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get(pane_id)?;
        fs_path_matches(&session.path, path).then_some(session.watch_revision.0)
    }

    /// Applies a compacted, resolved set of direct-child mutations (already
    /// re-read from disk by the caller — see
    /// [`crate::watch::coordinator::CompactedBatch::Targeted`]) to the active
    /// session for `pane_id`, if any, and if the on-disk `path` still matches
    /// that session's path (a stale/superseded watch reporting against an old
    /// path is a silent no-op, per Functional Requirement 5's "only current
    /// generations may publish").
    ///
    /// Updates the unfiltered snapshot, re-derives the view, bumps the watch
    /// and view revisions, and classifies the resulting change into the
    /// smallest applicable [`SessionPatch`] (`delta` for one unambiguous
    /// row move, `replace-view` otherwise). Returns `None` if there is no
    /// active session for `pane_id`, the path does not match, or nothing
    /// actually changed (e.g. a duplicate/no-op mutation report).
    pub fn apply_watch_mutation(
        &self,
        pane_id: &PaneId,
        path: &str,
        changed_entries: Vec<DirectoryEntry>,
        removed_paths: Vec<String>,
    ) -> Option<SessionPatch> {
        let mut sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get_mut(pane_id)?;
        if !fs_path_matches(&session.path, path) {
            return None;
        }

        let previous_baseline = session.baseline();
        let previous_view_rows = session.view.rows().to_vec();

        // Fold the mutation into the unfiltered snapshot: remove
        // `removed_paths`, upsert `changed_entries` by path.
        let removed_set: std::collections::HashSet<&str> =
            removed_paths.iter().map(String::as_str).collect();
        session
            .snapshot
            .retain(|entry| !removed_set.contains(entry.path.as_str()));
        for changed in changed_entries {
            if let Some(existing) = session
                .snapshot
                .iter_mut()
                .find(|entry| entry.path == changed.path)
            {
                *existing = changed;
            } else {
                session.snapshot.push(changed);
            }
        }

        session.view = SessionView::derive(&session.snapshot, &session.view_params);
        let next_view_rows = session.view.rows().to_vec();

        if previous_view_rows == next_view_rows {
            // Nothing observably changed in the derived view (e.g. a
            // metadata write that does not affect any filtered/sorted
            // field) — no patch to publish, and no revision to bump since
            // nothing downstream needs to reject anything as stale.
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

    /// Applies a `metadata-only` update (Phase 8 reuse target): fields on an
    /// already-visible row change without affecting sort/filter membership
    /// or position. The caller is responsible for confirming the change does
    /// not alter ordering (e.g. it does not touch the active sort key's
    /// field) — this method does not itself re-derive the view or bump any
    /// revision, matching `SessionPatch::MetadataOnly`'s "no baseline advance
    /// required" contract.
    pub fn apply_metadata_only(
        &self,
        pane_id: &PaneId,
        path: &str,
        updates: Vec<MetadataDelta>,
    ) -> Option<SessionPatch> {
        if updates.is_empty() {
            return None;
        }
        let sessions = self.sessions.lock().expect("directory session lock");
        let session = sessions.get(pane_id)?;
        if !fs_path_matches(&session.path, path) {
            return None;
        }

        Some(SessionPatch::MetadataOnly {
            pane_id: pane_id.clone(),
            tab_id: session.key.tab_id.clone(),
            path: session.path.clone(),
            baseline: session.baseline(),
            updates,
        })
    }

    /// Forces a `replace-view` patch for `pane_id` by resnapshotting the
    /// directory from disk (used when a watch coordinator overflow marks a
    /// watch dirty — Functional Requirement 2's "schedule one authoritative
    /// resnapshot"). Returns `None` if there is no active session or the
    /// resnapshot fails (the caller should fall back to leaving the session
    /// as-is; a later navigation/explicit refresh will recover it).
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

/// Loose path-equality helper for matching a watch's reported directory
/// against a session's stored (already-canonicalized/display) path. Exact
/// match first, falling back to `fs::display_path_from_path` normalization
/// on both sides so a caller passing a lexically-different-but-equivalent
/// path (e.g. differing trailing separator) is still recognized — mirrors
/// the exact-first/compatibility-fallback rule CLAUDE.md's Refresh Model
/// section requires.
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

/// Enumerates `path` non-recursively into an unfiltered [`DirectoryEntry`]
/// list suitable for session snapshot storage. Deliberately reads *every*
/// entry (ignoring `view.filter`/`view.show_hidden`) because the session
/// snapshot must stay valid across later view-only filter/show-hidden
/// changes without re-enumerating; the returned entries are always filtered
/// again by [`SessionView::derive`].
fn enumerate_snapshot(path: &Path, view: &ViewParams) -> Result<Vec<DirectoryEntry>, FsError> {
    let options = fs::ListDirOptions {
        path: path.to_string_lossy().into_owned(),
        sort_key: view.sort_key,
        sort_direction: view.sort_direction,
        // Enumerate unfiltered/all-hidden so the stored snapshot supports
        // every later view revision without re-reading the directory.
        filter: String::new(),
        show_hidden: true,
        include_item_counts: view.include_item_counts,
    };
    fs::list_dir(&options).map(|response| response.entries)
}

/// Reconciles a capture window (mutations observed between `begin_capture`
/// and the end of enumeration) against the freshly-enumerated `entries`.
///
/// Because `enumerate_snapshot` always reads the live directory, `entries`
/// already reflects the post-mutation state for any change that completed
/// before enumeration finished. The remaining risk is a mutation whose event
/// could not be resolved into a definite path set (rename pairs it could not
/// decode, `need_rescan`, or capture setup itself failing) — in that case we
/// resnapshot once more to guarantee no change is silently lost, and bump the
/// watch revision to mark the fresh baseline.
fn reconcile_capture(
    path: &Path,
    view: &ViewParams,
    entries: Vec<DirectoryEntry>,
    capture: Option<CaptureHandle>,
) -> Result<(Vec<DirectoryEntry>, WatchRevision), String> {
    let Some(capture) = capture else {
        // Capture could not be established (e.g. watch limits, unsupported
        // path). The enumeration above is still the best available
        // authoritative read; treat this as the initial watch revision but
        // note it never captured anything to reconcile.
        return Ok((entries, WatchRevision(0)));
    };

    resolve_reconciliation(
        path,
        view,
        entries,
        watch::drain_captured_mutations(capture),
    )
}

/// Pure reconciliation decision given the drained mutation set (or `None` for
/// an unresolvable capture). Separated from [`reconcile_capture`] so the
/// branching logic can be exercised deterministically under `test-utils`
/// without depending on real OS watcher event timing.
fn resolve_reconciliation(
    path: &Path,
    view: &ViewParams,
    entries: Vec<DirectoryEntry>,
    drained: Option<std::collections::HashSet<PathBuf>>,
) -> Result<(Vec<DirectoryEntry>, WatchRevision), String> {
    match drained {
        Some(mutated) if mutated.is_empty() => Ok((entries, WatchRevision(0))),
        Some(_) | None => {
            // At least one direct-child mutation raced enumeration (a
            // resolved mutation set), or capture could not tell us
            // definitively what changed at all (need_rescan/Any/Other/
            // unresolvable rename — the `None` case). Either way, resnapshot
            // once more as a final barrier rather than trusting `entries`:
            // `enumerate_snapshot`'s directory read is not guaranteed atomic
            // against concurrent mutations, so a mutation that fires
            // mid-scan (neither strictly before nor strictly after the read)
            // can race the enumeration and be missing from `entries` even
            // though capture observed it. The resnapshot closes that window
            // and guarantees the published baseline is authoritative; bump
            // the watch revision to mark it.
            let resnapshot = enumerate_snapshot(path, view)
                .map_err(|error| format!("Failed to resnapshot \"{}\": {error}", path.display()))?;
            Ok((resnapshot, WatchRevision(1)))
        }
    }
}

fn display_path(path: &Path) -> String {
    fs::canonicalize_dir(path)
        .map(|canonical| fs::display_path_from_path(&canonical))
        .unwrap_or_else(|_| fs::display_path_from_path(path))
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn enumerate_snapshot_for_tests(
    path: &Path,
    view: &ViewParams,
) -> Result<Vec<DirectoryEntry>, FsError> {
    enumerate_snapshot(path, view)
}

/// Exercises the watch-first reconciliation decision deterministically: given
/// a caller-supplied (possibly stale) `entries` and a caller-chosen drained
/// mutation outcome, proves a non-empty/ambiguous outcome always resnapshots
/// from disk rather than trusting `entries`, closing the `read_dir`
/// non-atomicity race window a real `CaptureHandle` cannot deterministically
/// reproduce in a test.
#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn resolve_reconciliation_for_tests(
    path: &Path,
    view: &ViewParams,
    entries: Vec<DirectoryEntry>,
    drained: Option<std::collections::HashSet<PathBuf>>,
) -> Result<(Vec<DirectoryEntry>, WatchRevision), String> {
    resolve_reconciliation(path, view, entries, drained)
}
