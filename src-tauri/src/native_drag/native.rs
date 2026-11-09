use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

fn drag_image() -> drag::Image {
    drag::Image::Raw(include_bytes!("../../icons/32x32.png").to_vec())
}

fn signal(sender: &Mutex<Option<oneshot::Sender<()>>>) {
    if let Ok(mut slot) = sender.lock() {
        if let Some(sender) = slot.take() {
            let _ = sender.send(());
        }
    }
}

fn run_session(
    window: tauri::WebviewWindow,
    paths: Vec<std::path::PathBuf>,
    sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
) {
    log::info!("native drag: starting session for {} path(s)", paths.len());
    let finished = Arc::clone(&sender);
    let started = drag::start_drag(
        &window,
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
            mode: drag::DragMode::Copy,
            ..Default::default()
        },
    );
    if let Err(error) = started {
        log::error!("Failed to start the native drag: {error}");
        signal(&sender);
    }
}

#[cfg(windows)]
type WindowsDragJob = (
    tauri::WebviewWindow,
    Vec<std::path::PathBuf>,
    Arc<Mutex<Option<oneshot::Sender<()>>>>,
    u32,
);

#[cfg(windows)]
struct WindowsDragWorker {
    sender: std::sync::mpsc::Sender<WindowsDragJob>,
    thread_id: u32,
}

#[cfg(windows)]
fn start_windows_drag(
    window: tauri::WebviewWindow,
    paths: Vec<std::path::PathBuf>,
    finished: Arc<Mutex<Option<oneshot::Sender<()>>>>,
) -> Result<(), String> {
    use std::sync::OnceLock;
    use windows::Win32::{
        System::Threading::AttachThreadInput, UI::WindowsAndMessaging::GetWindowThreadProcessId,
    };

    static WORKER: OnceLock<Result<WindowsDragWorker, String>> = OnceLock::new();
    let worker = WORKER
        .get_or_init(spawn_windows_drag_worker)
        .as_ref()
        .map_err(Clone::clone)?;

    let hwnd = window
        .hwnd()
        .map_err(|error| format!("Failed to resolve the native drag window: {error}"))?;
    let ui_thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
    if ui_thread == 0 {
        return Err("Failed to resolve the native drag window thread".to_string());
    }
    if !unsafe { AttachThreadInput(worker.thread_id, ui_thread, true) }.as_bool() {
        return Err("Failed to attach the native drag input queue".to_string());
    }
    if worker
        .sender
        .send((window, paths, finished, ui_thread))
        .is_err()
    {
        let _ = unsafe { AttachThreadInput(worker.thread_id, ui_thread, false) };
        return Err("native drag worker stopped unexpectedly".to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn spawn_windows_drag_worker() -> Result<WindowsDragWorker, String> {
    use std::sync::mpsc;

    let (sender, receiver) = mpsc::channel();
    let (ready, thread_id) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("native-drag".to_string())
        .spawn(move || run_windows_drag_worker(&receiver, &ready))
        .map_err(|error| format!("Failed to start the native drag worker: {error}"))?;
    let thread_id = thread_id
        .recv()
        .map_err(|_| "native drag worker stopped during initialization".to_string())?;
    Ok(WindowsDragWorker { sender, thread_id })
}

#[cfg(windows)]
fn run_windows_drag_worker(
    receiver: &std::sync::mpsc::Receiver<WindowsDragJob>,
    ready: &std::sync::mpsc::SyncSender<u32>,
) {
    use windows::Win32::{
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_NOREMOVE},
    };

    let mut message = MSG::default();
    unsafe {
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
    }
    let id = unsafe { GetCurrentThreadId() };
    if ready.send(id).is_err() {
        return;
    }
    while let Ok((window, paths, finished, ui_thread)) = receiver.recv() {
        run_session(window, paths, finished);
        if !unsafe { AttachThreadInput(id, ui_thread, false) }.as_bool() {
            log::warn!("Failed to detach native drag input queues");
        }
    }
}

pub async fn start(
    window: tauri::WebviewWindow,
    paths: Vec<std::path::PathBuf>,
) -> Result<(), String> {
    let (sender, receiver) = oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    let drag_window = window.clone();

    #[cfg(target_os = "macos")]
    window
        .run_on_main_thread(move || run_session(drag_window, paths, sender))
        .map_err(|error| format!("Failed to start the native drag: {error}"))?;

    #[cfg(windows)]
    start_windows_drag(drag_window, paths, sender)?;

    let mut receiver = receiver;
    let mut streamed = 0_u32;
    loop {
        match tokio::time::timeout(CURSOR_POLL_INTERVAL, &mut receiver).await {
            Ok(ended) => {
                emit_position(&window);
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

const CURSOR_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(4);

#[cfg(target_os = "macos")]
fn modifiers() -> super::DragModifiers {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags};

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
            cursor_x: cursor.x - f64::from(origin.x),
            cursor_y: cursor.y - f64::from(origin.y),
            frame_width: f64::from(frame.width),
            frame_height: f64::from(frame.height),
            modifiers: modifiers(),
        },
    );
}
