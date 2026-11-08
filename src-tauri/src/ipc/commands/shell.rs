use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem;
use super::logged_error;
use crate::ipc::mock;
use crate::ipc::types::{
    CancelThumbnailsRequest, InitialShellResponse, OpenPathRequest, RequestThumbnailsRequest,
    RequestThumbnailsResponse, SetWindowThemeRequest,
};
use std::path::Path;
#[cfg(not(feature = "test-utils"))]
use tauri::AppHandle;
use tauri::State;

#[tauri::command]
pub fn get_initial_shell() -> InitialShellResponse {
    mock::initial_shell()
}

#[tauri::command]
pub fn request_thumbnails(
    payload: RequestThumbnailsRequest,
    state: State<'_, Arc<crate::thumbnails::ThumbnailService>>,
) -> Result<RequestThumbnailsResponse, String> {
    crate::thumbnails::request::request_thumbnails(&state, payload)
}

#[tauri::command]
pub fn cancel_thumbnails(
    payload: CancelThumbnailsRequest,
    state: State<'_, Arc<crate::thumbnails::ThumbnailService>>,
) {
    state.cancel(
        &payload.pane_id,
        &payload.tab_id,
        &payload.path,
        payload.generation,
    );
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn open_path(
    payload: OpenPathRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    let key = payload.path.clone();
    latency_filesystem(&executor, key, move || open_path_impl(payload)).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn open_path(payload: OpenPathRequest) -> Result<(), String> {
    open_path_impl(payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn restart_as_admin(app: AppHandle) -> Result<(), String> {
    restart_as_admin_impl()?;
    // Only quit after the elevated instance started. A refused UAC prompt must
    // leave the running application alone.
    app.exit(0);
    Ok(())
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn restart_as_admin() -> Result<(), String> {
    restart_as_admin_impl()
}

#[tauri::command]
pub fn open_full_disk_access_settings() -> Result<(), String> {
    crate::launch::open_full_disk_access_settings()
        .map_err(|error| logged_error(format!("Failed to open Full Disk Access settings: {error}")))
}

/// Keeps the native title bar in step with the app theme.
#[tauri::command]
pub fn set_window_theme<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    payload: SetWindowThemeRequest,
) -> Result<(), String> {
    window
        .set_theme(crate::window_chrome::native_theme(payload.theme.as_deref()))
        .map_err(|error| format!("Failed to set the window theme: {error}"))
}

/// `mica` when the window got the Windows 11 backdrop, `solid` otherwise.
#[tauri::command]
pub fn get_window_backdrop() -> crate::window_chrome::WindowBackdrop {
    crate::window_chrome::current_backdrop()
}

fn restart_as_admin_impl() -> Result<(), String> {
    crate::launch::restart_as_admin().map_err(|error| {
        logged_error(format!(
            "Failed to restart with administrator permissions: {error}"
        ))
    })
}

fn open_path_impl(payload: OpenPathRequest) -> Result<(), String> {
    crate::launch::open_path(Path::new(&payload.path))
        .map_err(|error| logged_error(format!("Failed to open \"{}\": {error}", payload.path)))
}
