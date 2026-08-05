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

/// Cursor position during a live drag session, in CSS pixels relative to the
/// webview's top-left corner.
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
mod native {
    use std::sync::{Arc, Mutex};

    use tokio::sync::oneshot;

    /// The app icon doubles as the drag preview. `drag` requires an image and this
    /// one is already in the source tree, so embedding it avoids resolving a
    /// bundled resource at runtime (which differs per platform and per installer).
    fn drag_image() -> drag::Image {
        drag::Image::Raw(include_bytes!("../icons/32x32.png").to_vec())
    }

    /// `drag`'s completion callback is `Fn`, so the one-shot sender lives behind a
    /// mutex and is consumed by whichever path fires first: the drag finishing, or
    /// the session failing to start at all.
    fn signal(sender: &Mutex<Option<oneshot::Sender<()>>>) {
        if let Ok(mut slot) = sender.lock() {
            if let Some(sender) = slot.take() {
                let _ = sender.send(());
            }
        }
    }

    pub async fn start(
        window: tauri::WebviewWindow,
        paths: Vec<std::path::PathBuf>,
    ) -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let finished = Arc::clone(&sender);
        let drag_window = window.clone();

        // Both `beginDraggingSession` (macOS) and `DoDragDrop` (Windows) must run on
        // the UI thread; the latter also blocks it for the duration of the gesture,
        // pumping its own modal message loop.
        window
            .run_on_main_thread(move || {
                log::info!("native drag: starting session for {} path(s)", paths.len());
                let started = drag::start_drag(
                    &drag_window,
                    drag::DragItem::Files(paths),
                    drag_image(),
                    move |result, cursor| {
                        log::info!(
                            "native drag: session ended {result:?} at ({}, {})",
                            cursor.x,
                            cursor.y
                        );
                        signal(&finished)
                    },
                    drag::Options {
                        // External targets always receive a copy: a `Move` the OS
                        // honours would delete the file out from under the pane.
                        mode: drag::DragMode::Copy,
                        ..Default::default()
                    },
                );
                if let Err(error) = started {
                    log::error!("Failed to start the native drag: {error}");
                    signal(&sender);
                }
            })
            .map_err(|error| format!("Failed to start the native drag: {error}"))?;

        // WKWebView does not deliver a self-originated drag session back to the page
        // as DOM drag events, so the webview is blind for the whole gesture. Streaming
        // the cursor position is what lets the frontend light up drop targets and
        // resolve the drop itself; see `native-drag-bridge.ts`.
        let mut receiver = receiver;
        let mut streamed = 0_u32;
        loop {
            match tokio::time::timeout(CURSOR_POLL_INTERVAL, &mut receiver).await {
                // The session ended. One last position first, so the drop lands where
                // the pointer actually stopped rather than one tick behind it.
                Ok(ended) => {
                    emit_position(&window);
                    // In-app drops depend entirely on this stream, and a stream that
                    // never ran looks exactly like a drag that hit nothing. One line
                    // per gesture makes the difference visible without a debug build.
                    log::info!("native drag: session ended after {streamed} positions");
                    return ended.map_err(|_| "native drag ended unexpectedly".to_string());
                }
                Err(_) => {
                    emit_position(&window);
                    streamed += 1;
                }
            }
        }
    }

    /// ~60 Hz: fast enough that the drop-target highlight tracks the pointer without
    /// visible lag, slow enough to stay a rounding error next to the drag itself.
    const CURSOR_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

    /// Reads the modifier keys currently held. Polled alongside the cursor rather
    /// than sampled at drag start, so pressing (or releasing) a modifier mid-drag
    /// changes copy-vs-move exactly as it does for a webview drag.
    #[cfg(target_os = "macos")]
    fn modifiers() -> super::DragModifiers {
        use objc2_app_kit::{NSEvent, NSEventModifierFlags};

        // A class method with no `MainThreadMarker` argument: readable from this
        // polling thread while the main thread is busy running the drag session.
        let flags = NSEvent::modifierFlags_class();
        super::DragModifiers {
            ctrl_key: flags.contains(NSEventModifierFlags::Control),
            shift_key: flags.contains(NSEventModifierFlags::Shift),
            alt_key: flags.contains(NSEventModifierFlags::Option),
            meta_key: flags.contains(NSEventModifierFlags::Command),
        }
    }

    #[cfg(windows)]
    fn modifiers() -> super::DragModifiers {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_SHIFT,
        };

        // `GetAsyncKeyState` reads global key state, unlike `GetKeyState`, which is
        // scoped to the calling thread's input queue — and `DoDragDrop` is holding
        // the main thread's queue for the whole gesture.
        fn held(key: i32) -> bool {
            (unsafe { GetAsyncKeyState(key) } as u16 & 0x8000) != 0
        }

        super::DragModifiers {
            ctrl_key: held(VK_CONTROL as i32),
            shift_key: held(VK_SHIFT as i32),
            alt_key: held(VK_MENU as i32),
            meta_key: held(VK_LWIN as i32),
        }
    }

    /// Reports the cursor in CSS pixels relative to the webview's top-left, which is
    /// the coordinate space `document.elementFromPoint` expects. A cursor outside the
    /// window simply lands outside the viewport, and the frontend hit-test misses —
    /// which is exactly right for a drop into another application.
    fn emit_position(window: &tauri::WebviewWindow) {
        let (Ok(cursor), Ok(origin), Ok(frame)) = (
            window.cursor_position(),
            window.outer_position(),
            window.outer_size(),
        ) else {
            return;
        };
        let _ = tauri::Emitter::emit(
            window,
            "drag://position",
            super::DragPositionEvent {
                // Relative to the window frame; the frontend removes the chrome.
                // Deliberately not derived from `window.screenX/screenY` either:
                // WKWebView reports a stale origin there (measured 0,1890 for a
                // window actually at 1052,418), yielding negative coordinates.
                cursor_x: cursor.x - f64::from(origin.x),
                cursor_y: cursor.y - f64::from(origin.y),
                frame_width: f64::from(frame.width),
                frame_height: f64::from(frame.height),
                modifiers: modifiers(),
            },
        );
    }
}

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
