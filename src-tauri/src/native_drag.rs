use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartNativeDragRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragPositionEvent {
    pub cursor_x: f64,
    pub cursor_y: f64,
    pub frame_width: f64,
    pub frame_height: f64,
    #[serde(flatten)]
    pub modifiers: DragModifiers,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragModifiers {
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
}

pub fn drag_paths(payload: &StartNativeDragRequest) -> Result<Vec<PathBuf>, String> {
    if payload.paths.is_empty() {
        return Err("native drag requires at least one path".to_string());
    }

    let mut paths = Vec::with_capacity(payload.paths.len());
    for path in &payload.paths {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(format!(
                "native drag requires absolute paths, got \"{}\"",
                path.display()
            ));
        }
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(all(not(feature = "test-utils"), any(windows, target_os = "macos")))]
#[path = "native_drag/native.rs"]
mod native;

#[cfg(all(not(feature = "test-utils"), any(windows, target_os = "macos")))]
#[tauri::command]
pub async fn start_native_drag(
    window: tauri::WebviewWindow,
    payload: StartNativeDragRequest,
) -> Result<(), String> {
    native::start(window, drag_paths(&payload)?).await
}

#[cfg(any(feature = "test-utils", not(any(windows, target_os = "macos"))))]
#[tauri::command]
pub fn start_native_drag(payload: StartNativeDragRequest) -> Result<(), String> {
    drag_paths(&payload).map(|_| ())
}
