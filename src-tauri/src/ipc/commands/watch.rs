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

pub fn fold_dir_patch_into_session(
    session_state: &crate::directory_session::DirectorySessionService,
    item_count_state: &ItemCountService,
    patch: &crate::watch::DirPatch,
) -> Option<crate::ipc::types::SessionPatch> {
    fold_dir_patch_into_session_core(session_state, item_count_state, patch)
}

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
