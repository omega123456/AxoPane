//! Hands a pane selection to the OS as a real drag session so it can be dropped
//! into other applications (Teams, Finder, Explorer, …).
//!
//! The webview's own HTML5 drag never becomes an OS-level drag, so the frontend
//! cancels it and calls this command instead. In-app drop targets are unaffected:
//! they read the drag payload from the frontend store, and the OS drag session is
//! delivered back into the webview as ordinary DOM drag events.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartNativeDragRequest {
    pub paths: Vec<String>,
}

/// Cursor position during a live drag session, in **physical** pixels relative to
/// the window frame's top-left, alongside the frame's own physical size.
///
/// Everything is left in physical pixels on purpose. The frontend converts using
/// its own `devicePixelRatio` and derives the window chrome by comparing the frame
/// against its viewport, so the mapping stays correct without hardcoded constants
/// and without assuming a zoom level. `inner_position`/`inner_size` are unusable
/// here: tao returns frame geometry for both (measured `inner == outer` on macOS),
/// which silently left every position one title bar too low.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragPositionEvent {
    pub cursor_x: f64,
    pub cursor_y: f64,
    pub frame_width: f64,
    pub frame_height: f64,
    /// Live modifier state. The OS owns the keyboard for the duration of the drag,
    /// so the webview sees no key events — without this, copy-vs-move could only
    /// honour modifiers already held when the drag started.
    #[serde(flatten)]
    pub modifiers: DragModifiers,
}

/// Modifier keys as the DOM names them, so synthesized drag events carry the same
/// flags a real one would.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragModifiers {
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
}

/// Validates the request and projects it onto real paths. Shared by both builds
/// so the request contract is identical with and without a native drag session.
pub fn drag_paths(payload: &StartNativeDragRequest) -> Result<Vec<PathBuf>, String> {
    if payload.paths.is_empty() {
        return Err("native drag requires at least one path".to_string());
    }

    let mut paths = Vec::with_capacity(payload.paths.len());
    for path in &payload.paths {
        let path = PathBuf::from(path);
        // `drag` requires absolute paths; a relative one would resolve against the
        // process working directory in whichever app receives the drop.
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

/// A drag session is process-global and modal, so tests (and platforms without a
/// native implementation) validate the request contract and stop there.
#[cfg(any(feature = "test-utils", not(any(windows, target_os = "macos"))))]
#[tauri::command]
pub fn start_native_drag(payload: StartNativeDragRequest) -> Result<(), String> {
    drag_paths(&payload).map(|_| ())
}
