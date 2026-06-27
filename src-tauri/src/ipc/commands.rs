use crate::fs;
use crate::ops::{OpSnapshot, OpsService};
use crate::persist::PersistenceState;
use crate::size::SizeService;
use crate::volumes;
use crate::watch::WatchService;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::mock;
use super::types::{
    AppConfig, CancelSizeRequest, CancelSizeResponse, CreateEntryRequest, DeleteEntriesRequest,
    FolderSizeRequest, FolderSizesRequest, InitialShellResponse, ListDirRequest, ListDirResponse,
    LogFrontendRequest, OpIdRequest, OpenPathRequest, RefreshTabRequest, RenameEntryRequest,
    ReorderOpsRequest, ResolveConflictRequest, SaveConfigRequest, SaveSessionRequest, SessionState,
    SetTabWatchRequest, SizeStateEvent, StartOpRequest, VolumeInfo,
};
use crate::fs::DirectoryEntry;
use std::path::Path;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter};
use tauri::State;

#[cfg(feature = "test-utils")]
fn noop_dir_patch(_: crate::watch::DirPatch) {}

#[cfg(feature = "test-utils")]
fn noop_watch_error(_: String, _: String) {}

#[tauri::command]
pub fn get_initial_shell() -> InitialShellResponse {
    mock::initial_shell()
}

#[tauri::command]
pub fn list_dir(payload: ListDirRequest) -> Result<ListDirResponse, String> {
    fs::list_dir(&payload).map_err(|error| {
        let message = format!("Failed to load \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn create_folder(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    fs::create_directory(&payload.parent, &payload.name).map_err(|error| {
        let message = format!("Failed to create folder \"{}\": {error}", payload.name);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn create_file(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    fs::create_file(&payload.parent, &payload.name).map_err(|error| {
        let message = format!("Failed to create file \"{}\": {error}", payload.name);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn rename_entry(payload: RenameEntryRequest) -> Result<DirectoryEntry, String> {
    fs::rename_entry(&payload.path, &payload.new_name).map_err(|error| {
        let message = format!("Failed to rename \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn delete_entries(payload: DeleteEntriesRequest) -> Result<(), String> {
    fs::delete_entries(&payload.paths).map_err(|error| {
        let message = format!("Failed to delete items: {error}");
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn open_path(payload: OpenPathRequest) -> Result<(), String> {
    crate::launch::open_path(Path::new(&payload.path)).map_err(|error| {
        let message = format!("Failed to open \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn list_volumes(app: AppHandle) -> Result<Vec<VolumeInfo>, String> {
    let volumes = volumes::list_volumes();

    app.emit(
        super::events::VOLUMES_CHANGED,
        super::types::VolumesChangedEvent {
            volumes: volumes.clone(),
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(volumes)
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_volumes() -> Result<Vec<VolumeInfo>, String> {
    Ok(volumes::list_volumes())
}

#[tauri::command]
pub fn everything_status(state: State<'_, SizeService>) -> super::types::EverythingStatus {
    state.everything_status()
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_folder_size(
    payload: FolderSizeRequest,
    app: AppHandle,
    state: State<'_, SizeService>,
) {
    request_folder_sizes(
        FolderSizesRequest {
            paths: vec![payload.path],
        },
        app,
        state,
    );
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_folder_size(
    payload: FolderSizeRequest,
    state: State<'_, SizeService>,
) -> Vec<SizeStateEvent> {
    request_folder_sizes(
        FolderSizesRequest {
            paths: vec![payload.path],
        },
        state,
    )
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_folder_sizes(
    payload: FolderSizesRequest,
    app: AppHandle,
    state: State<'_, SizeService>,
) {
    let app_handle = app.clone();
    state.request_paths(payload.paths, move |update| {
        let _ = app_handle.emit(
            super::events::SIZE_STATE,
            SizeStateEvent {
                path: update.path,
                state: update.state,
                source: update.source,
                size_bytes: update.size_bytes,
            },
        );
    });
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_folder_sizes(
    payload: FolderSizesRequest,
    state: State<'_, SizeService>,
) -> Vec<SizeStateEvent> {
    let updates = Arc::new(Mutex::new(Vec::<SizeStateEvent>::new()));
    let updates_for_emitter = Arc::clone(&updates);
    let expected_paths = payload.paths.len().max(1);

    state.request_paths(payload.paths, move |update| {
        updates_for_emitter
            .lock()
            .expect("size updates lock")
            .push(SizeStateEvent {
                path: update.path,
                state: update.state,
                source: update.source,
                size_bytes: update.size_bytes,
            });
    });

    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        let recorded = updates.lock().expect("size updates lock");
        let terminal_count = recorded
            .iter()
            .filter(|event| {
                matches!(
                    event.state,
                    crate::size::SizeStateKind::Ready
                        | crate::size::SizeStateKind::Error
                        | crate::size::SizeStateKind::Na
                )
            })
            .count();
        if terminal_count >= expected_paths {
            return recorded.clone();
        }
        drop(recorded);
        std::thread::sleep(Duration::from_millis(10));
    }

    let recorded = updates.lock().expect("size updates lock").clone();
    recorded
}

#[tauri::command]
pub fn cancel_size(
    payload: CancelSizeRequest,
    state: State<'_, SizeService>,
) -> CancelSizeResponse {
    CancelSizeResponse {
        cancelled: state.cancel(&payload.path),
    }
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    app: AppHandle,
    state: State<'_, WatchService>,
) -> Result<(), String> {
    let patch_app = app.clone();
    let error_app = app;

    state.set_tab_watch(
        payload.target,
        Arc::new(move |patch| {
            let _ = patch_app.emit(
                super::events::DIR_PATCH,
                super::types::DirPatchEvent {
                    tab_id: patch.tab_id,
                    path: patch.path,
                    reason: patch.reason,
                    changed: patch
                        .changed
                        .into_iter()
                        .map(|change| super::types::DirEntryPatch {
                            path: change.path,
                            entry: change.entry,
                        })
                        .collect(),
                    removed: patch.removed,
                },
            );
        }),
        Arc::new(move |path, message| {
            log::warn!("watch error for {path}: {message}");
            let _ = error_app.emit(
                super::events::WATCH_ERROR,
                super::types::WatchErrorEvent { path, message },
            );
        }),
    )
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    state: State<'_, WatchService>,
) -> Result<(), String> {
    state.set_tab_watch(
        payload.target,
        Arc::new(noop_dir_patch),
        Arc::new(noop_watch_error),
    )
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn refresh_tab(
    payload: RefreshTabRequest,
    app: AppHandle,
    state: State<'_, WatchService>,
) -> Result<super::types::DirPatchEvent, String> {
    let patch = state.refresh_tab(payload.target, Arc::new(|_| {}))?;
    let event = super::types::DirPatchEvent {
        tab_id: patch.tab_id,
        path: patch.path,
        reason: patch.reason,
        changed: patch
            .changed
            .into_iter()
            .map(|change| super::types::DirEntryPatch {
                path: change.path,
                entry: change.entry,
            })
            .collect(),
        removed: patch.removed,
    };

    app.emit(super::events::DIR_PATCH, event.clone())
        .map_err(|error| error.to_string())?;

    Ok(event)
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn refresh_tab(
    payload: RefreshTabRequest,
    state: State<'_, WatchService>,
) -> Result<super::types::DirPatchEvent, String> {
    let patch = state.refresh_tab(payload.target, Arc::new(noop_dir_patch))?;
    Ok(super::types::DirPatchEvent {
        tab_id: patch.tab_id,
        path: patch.path,
        reason: patch.reason,
        changed: patch
            .changed
            .into_iter()
            .map(|change| super::types::DirEntryPatch {
                path: change.path,
                entry: change.entry,
            })
            .collect(),
        removed: patch.removed,
    })
}

#[tauri::command]
pub fn start_op(payload: StartOpRequest, state: State<'_, OpsService>) -> String {
    state.start_op(payload)
}

#[tauri::command]
pub fn pause_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.pause_op(&payload.id);
}

#[tauri::command]
pub fn resume_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.resume_op(&payload.id);
}

#[tauri::command]
pub fn cancel_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.cancel_op(&payload.id);
}

#[tauri::command]
pub fn retry_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.retry_op(&payload.id);
}

#[tauri::command]
pub fn reorder_ops(payload: ReorderOpsRequest, state: State<'_, OpsService>) {
    state.reorder_ops(&payload.ids);
}

#[tauri::command]
pub fn resolve_conflict(payload: ResolveConflictRequest, state: State<'_, OpsService>) {
    state.resolve_conflict(
        &payload.id,
        payload.resolution,
        payload.apply_to_all,
        payload.rename_to,
    );
}

#[tauri::command]
pub fn queue_snapshot(state: State<'_, OpsService>) -> Vec<OpSnapshot> {
    state.snapshot()
}

#[tauri::command]
pub fn has_unfinished_ops(state: State<'_, OpsService>) -> bool {
    state.has_unfinished_work()
}

#[tauri::command]
pub fn load_config(state: State<'_, PersistenceState>) -> AppConfig {
    state.config.current()
}

#[tauri::command]
pub fn save_config(payload: SaveConfigRequest, state: State<'_, PersistenceState>) -> AppConfig {
    state.config.replace(payload.config.clone());
    payload.config
}

#[tauri::command]
pub fn load_session(state: State<'_, PersistenceState>) -> SessionState {
    state.session.current()
}

#[tauri::command]
pub fn save_session(
    payload: SaveSessionRequest,
    state: State<'_, PersistenceState>,
) -> SessionState {
    state.session.replace(payload.session.clone());
    payload.session
}

/// Map a frontend level string to a [`log::Level`].
///
/// The frontend permits `trace`, which is folded into `Debug`. Unknown values
/// default to `Info`.
pub fn frontend_log_level(level: &str) -> log::Level {
    match level {
        "error" => log::Level::Error,
        "warn" => log::Level::Warn,
        "debug" | "trace" => log::Level::Debug,
        _ => log::Level::Info,
    }
}

/// Compose a single log line from a frontend log request. `category` defaults to
/// `frontend`; serialized `details` (when present) are appended to the message.
pub fn format_frontend_log(category: Option<&str>, message: &str, details: Option<&str>) -> String {
    let category = category.unwrap_or("frontend");
    match details {
        Some(details) => format!("[{category}] {message} {details}"),
        None => format!("[{category}] {message}"),
    }
}

/// Forward a frontend log line into the application logger (`tauri-plugin-log`).
///
/// The frontend logger (`app-log-commands.ts`) routes every line here so test
/// runs stay quiet (the line goes to the IPC sink, not the test console) and so
/// frontend diagnostics share the backend's stdout/webview log targets.
#[tauri::command]
pub fn log_frontend(payload: LogFrontendRequest) {
    let line = format_frontend_log(
        payload.category.as_deref(),
        &payload.message,
        payload.details.as_deref(),
    );
    log::log!(frontend_log_level(&payload.level), "{line}");
}
