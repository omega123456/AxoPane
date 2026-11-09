use crate::item_counts::ItemCountService;
#[cfg(feature = "test-utils")]
use crate::watch::WatchService;
use crate::watch::{TabWatchArm, WatchEmitters};
use std::sync::Arc;

use crate::fs::DirectoryEntry;
use crate::ipc::types::SetTabWatchRequest;
#[cfg(feature = "test-utils")]
use tauri::State;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter, Manager};

#[cfg(feature = "test-utils")]
#[inline(never)]
pub fn noop_dir_patch(_: crate::watch::DirPatch) {}

#[cfg(feature = "test-utils")]
#[inline(never)]
pub fn noop_watch_error(_: String, _: String) {}

/// Folds a legacy [`crate::watch::DirPatch`] into the v2
/// `DirectorySessionService`, if the reporting tab has an active v2 session
/// for the corresponding pane. Reuses `WatchService`'s
/// already-tested long-lived per-tab watch as the mutation source rather than
/// standing up a second independent long-lived watcher; the v2 session only
/// consumes the *result*, computing its own Rust-authoritative
/// [`SessionPatch`] rather than trusting the legacy patch's already-applied
/// v1 ordering. Returns `None` when there is no active v2 session for the
/// pane, the path does not match, or nothing changed in the derived view —
/// all silent no-ops, matching "stale/path-mismatched patches affect neither
/// pane nor tree" (this function only ever *adds* a session patch on top of
/// the legacy patch, never suppresses it).
pub fn fold_dir_patch_into_session(
    session_state: &crate::directory_session::DirectorySessionService,
    item_count_state: &ItemCountService,
    patch: &crate::watch::DirPatch,
) -> Option<crate::ipc::types::SessionPatch> {
    fold_dir_patch_into_session_core(session_state, item_count_state, patch)
}

/// Cfg-agnostic core shared by the real (non-`test-utils`) `set_tab_watch`
/// callback and [`fold_dir_patch_into_session_for_tests`], so the fold ->
/// invalidate pipeline is exercised identically by both — see
/// [`fold_dir_patch_into_session`]'s docs for the full contract.
fn fold_dir_patch_into_session_core(
    session_state: &crate::directory_session::DirectorySessionService,
    item_count_state: &ItemCountService,
    patch: &crate::watch::DirPatch,
) -> Option<crate::ipc::types::SessionPatch> {
    let pane_id = crate::watch::pane_scope(&patch.tab_id).to_string();
    let changed_entries: Vec<DirectoryEntry> = patch
        .changed
        .iter()
        .filter_map(|change| change.entry.clone())
        .collect();
    let changed_dirs: Vec<String> = patch
        .changed
        .iter()
        .filter_map(|change| {
            change
                .entry
                .as_ref()
                .filter(|entry| entry.is_dir)
                .map(|entry| entry.path.clone())
        })
        .collect();
    let target = crate::directory_session::PanePath {
        pane_id: &pane_id,
        path: &patch.path,
    };
    let mutation = crate::directory_session::WatchMutation {
        changed_entries,
        removed_paths: patch.removed.clone(),
    };
    let session_patch = session_state.apply_watch_mutation(target, mutation)?;
    invalidate_item_counts_for_patch(item_count_state, &session_patch, &changed_dirs);
    Some(session_patch)
}

/// Matches invalidation breadth to a folded [`crate::ipc::types::SessionPatch`]:
/// an unambiguous `Delta` (single direct-child add/remove/update) only makes
/// the mutated directory's own item count stale (its child count changed by
/// exactly one), so only that directory is invalidated. A `ReplaceView`
/// (multiple or order-ambiguous changes) additionally invalidates every
/// changed *directory* row itself, since a broader/ambiguous batch cannot
/// prove those child directories' own cached counts are still accurate
/// (e.g. a rename-in-place can carry a different identity than the cache
/// key remembers). `MetadataOnly` patches never touch child membership, so
/// they are intentionally left alone (Items counts are unaffected by a
/// field-only update).
///
/// The generation key is each patch's post-mutation `watch_revision` — a
/// per-session, mutation-driven counter that only advances when
/// `apply_watch_mutation`/`resnapshot` actually changes the session's
/// derived view (see `directory_session::mod`'s `ActiveSession::baseline`).
/// Unlike `process_automatic_request`'s `request_id` (a frontend-supplied
/// per-request counter that happens to always be "fresh" because every
/// viewport request gets a new one), `watch_revision` is the only signal in
/// this codebase that actually represents "this directory's on-disk content
/// generation as last observed by the owning session" — exactly the
/// invariant `ItemCountCache::invalidate_generation` needs to drop stale
/// counts without needing every viewport request to also relitigate
/// freshness.
fn invalidate_item_counts_for_patch(
    item_count_state: &ItemCountService,
    patch: &crate::ipc::types::SessionPatch,
    changed_dirs: &[String],
) {
    use crate::ipc::types::SessionPatch;

    match patch {
        SessionPatch::Delta {
            path,
            next_baseline,
            ..
        } => {
            item_count_state.invalidate_directory_generation(path, next_baseline.watch_revision.0);
        }
        SessionPatch::ReplaceView {
            path,
            next_baseline,
            ..
        } => {
            let generation = next_baseline.watch_revision.0;
            item_count_state.invalidate_directory_generation(path, generation);
            for changed_dir in changed_dirs {
                item_count_state.invalidate_directory_generation(changed_dir, generation);
            }
        }
        SessionPatch::MetadataOnly { .. } => {}
    }
}

/// Test-only entry point that exercises the exact same watch-patch ->
/// session-fold -> item-count-invalidation pipeline as the real (non-
/// `test-utils`) `set_tab_watch` callback above, without requiring a live
/// `AppHandle`/event emission. `fold_dir_patch_into_session` itself stays
/// `#[cfg(not(feature = "test-utils"))]`-gated because it is only ever
/// reached through the real Tauri command in production; this wrapper lets
/// integration tests prove the invalidation-breadth contract
/// (`invalidate_item_counts_for_patch`) against a real folded
/// [`crate::watch::patch::SessionPatch`] instead of constructing one by hand.
#[cfg(feature = "test-utils")]
pub fn fold_dir_patch_into_session_for_tests(
    session_state: &crate::directory_session::DirectorySessionService,
    item_count_state: &ItemCountService,
    patch: &crate::watch::DirPatch,
) -> Option<crate::ipc::types::SessionPatch> {
    fold_dir_patch_into_session_core(session_state, item_count_state, patch)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    app: AppHandle,
    services: crate::ipc::command_services::WatchServices<'_>,
) -> Result<(), String> {
    let emitters = WatchEmitters {
        patch: session_patch_emitter(app.clone(), Arc::clone(services.sessions.inner())),
        error: watch_error_emitter(app, "watch error"),
    };
    services
        .watches
        .set_tab_watch(tab_watch_arm(payload), emitters)
}

fn tab_watch_arm(payload: SetTabWatchRequest) -> TabWatchArm {
    TabWatchArm {
        target: payload.target,
        listing_seed_entries: None,
        entries: payload.entries,
    }
}

#[cfg(not(feature = "test-utils"))]
pub(crate) fn session_patch_emitter(
    app: AppHandle,
    session_state: Arc<crate::directory_session::DirectorySessionService>,
) -> Arc<dyn Fn(crate::watch::DirPatch) + Send + Sync> {
    Arc::new(move |patch| {
        let item_count_state = app.state::<ItemCountService>();
        if let Some(session_patch) =
            fold_dir_patch_into_session(&session_state, &item_count_state, &patch)
        {
            let _ = app.emit(crate::ipc::events::DIR_SESSION_PATCH, session_patch);
        }
    })
}

#[cfg(not(feature = "test-utils"))]
pub(crate) fn watch_error_emitter(
    app: AppHandle,
    log_context: &'static str,
) -> Arc<dyn Fn(String, String) + Send + Sync> {
    Arc::new(move |path, message| {
        log::warn!("{log_context} for {path}: {message}");
        let _ = app.emit(
            crate::ipc::events::WATCH_ERROR,
            crate::ipc::types::WatchErrorEvent { path, message },
        );
    })
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    state: State<'_, Arc<WatchService>>,
) -> Result<(), String> {
    let emitters = WatchEmitters {
        patch: Arc::new(noop_dir_patch),
        error: Arc::new(noop_watch_error),
    };
    state.set_tab_watch(tab_watch_arm(payload), emitters)
}
