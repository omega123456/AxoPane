//! Windows volume-change watcher: a dedicated thread owning a message-only
//! window registered for shell change notifications
//! (`SHChangeNotifyRegister`). `GetMessageW` blocks with zero idle wakeups
//! until the shell posts a notification, covering local, removable and
//! network drives — the same set the previous 1s poll loop covered.

use std::ptr;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    SHCNRF_InterruptLevel, SHCNRF_ShellLevel, SHChangeNotifyDeregister, SHChangeNotifyEntry,
    SHChangeNotifyRegister, SHCNE_DRIVEADD, SHCNE_DRIVEREMOVED, SHCNE_MEDIAINSERTED,
    SHCNE_MEDIAREMOVED, SHCNE_NETSHARE, SHCNE_NETUNSHARE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PostThreadMessageW, RegisterClassExW, RegisterWindowMessageW,
    SetWindowLongPtrW, TranslateMessage, GWLP_USERDATA, HWND_MESSAGE, MSG, WM_QUIT, WNDCLASSEXW,
    WNDCLASS_STYLES, WS_OVERLAPPED,
};

use super::registry::RegistryReconcileBridge;

const WINDOW_CLASS_NAME_STR: &str = "AxoPaneVolumeMonitorWindow";
const CHANGE_MESSAGE_NAME: &str = "AxoPaneVolumeMonitorChanged";

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Owns the message-only-window thread. Dropping posts `WM_QUIT` into the
/// thread's message queue (breaking the blocking `GetMessageW` loop), then
/// joins it once the thread has deregistered and torn down its window.
pub struct Watcher {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

struct ThreadContext {
    bridge: Arc<RegistryReconcileBridge>,
    change_message: u32,
}

impl Watcher {
    /// Starts the message-only-window thread that calls back into the
    /// volume registry on shell change notifications. `bridge` is kept
    /// alive for the watcher's lifetime by this thread's `ThreadContext`.
    pub fn start_for_registry(bridge: Arc<RegistryReconcileBridge>) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();

        let handle = thread::Builder::new()
            .name("volume-registry".to_string())
            .spawn(move || run(bridge, ready_tx))
            .map_err(|error| error.to_string())?;

        let thread_id = ready_rx
            .recv()
            .map_err(|_| "volume monitor thread exited before signaling readiness".to_string())??;

        Ok(Self {
            thread_id,
            handle: Some(handle),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // SAFETY: `thread_id` names the thread started in `start`, which is
        // still running its `GetMessageW` loop (or has already exited, in
        // which case posting is a harmless no-op).
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run(bridge: Arc<RegistryReconcileBridge>, ready: Sender<Result<u32, String>>) {
    // SAFETY: window class registration, window creation and shell
    // notification registration are only ever performed once per process,
    // synchronously, on this dedicated thread before any message is pumped.
    let setup = unsafe { setup_window_and_registration(bridge) };
    let (hwnd, notify_id, context_ptr) = match setup {
        Ok(parts) => parts,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    // SAFETY: called on the thread that owns `hwnd`'s message queue.
    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = ready.send(Ok(thread_id));

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is exclusively owned by this thread; blocks until a
        // message (including our shell notification, or `WM_QUIT` from
        // `Watcher::drop`) arrives.
        let has_message = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if has_message.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // SAFETY: `notify_id` came from a successful `SHChangeNotifyRegister`
    // call above and is deregistered exactly once, here, before teardown.
    unsafe {
        let _ = SHChangeNotifyDeregister(notify_id);
        drop(Box::from_raw(context_ptr));
        let _ = DestroyWindow(hwnd);
    }
}

/// # Safety
/// Must be called at most once, before any message loop for the created
/// window is running.
unsafe fn setup_window_and_registration(
    bridge: Arc<RegistryReconcileBridge>,
) -> Result<(HWND, u32, *mut ThreadContext), String> {
    let class_name = wide_null(WINDOW_CLASS_NAME_STR);
    let class_name_ptr = PCWSTR(class_name.as_ptr());

    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: WNDCLASS_STYLES(0),
        lpfnWndProc: Some(wnd_proc),
        lpszClassName: class_name_ptr,
        ..Default::default()
    };
    if RegisterClassExW(&class) == 0 {
        return Err("RegisterClassExW failed".to_string());
    }

    let hwnd = CreateWindowExW(
        Default::default(),
        class_name_ptr,
        class_name_ptr,
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        Some(HWND_MESSAGE),
        None,
        None,
        None,
    )
    .map_err(|error| error.to_string())?;

    let change_message_name = wide_null(CHANGE_MESSAGE_NAME);
    let change_message = RegisterWindowMessageW(PCWSTR(change_message_name.as_ptr()));
    if change_message == 0 {
        return Err("RegisterWindowMessageW failed".to_string());
    }

    let context = Box::into_raw(Box::new(ThreadContext {
        bridge,
        change_message,
    }));
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, context as isize);

    let entry = SHChangeNotifyEntry {
        pidl: ptr::null_mut(),
        fRecursive: true.into(),
    };
    let sources = SHCNRF_InterruptLevel.0 | SHCNRF_ShellLevel.0;
    let events = (SHCNE_DRIVEADD
        | SHCNE_DRIVEREMOVED
        | SHCNE_MEDIAINSERTED
        | SHCNE_MEDIAREMOVED
        | SHCNE_NETSHARE
        | SHCNE_NETUNSHARE)
        .0;

    let notify_id = SHChangeNotifyRegister(
        hwnd,
        windows::Win32::UI::Shell::SHCNRF_SOURCE(sources),
        events as i32,
        change_message,
        1,
        &entry,
    );
    if notify_id == 0 {
        drop(Box::from_raw(context));
        let _ = DestroyWindow(hwnd);
        return Err("SHChangeNotifyRegister failed".to_string());
    }

    Ok((hwnd, notify_id, context))
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let context_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ThreadContext;
    if !context_ptr.is_null() {
        let context = &*context_ptr;
        if msg == context.change_message {
            context.bridge.reconcile();
            return LRESULT(0);
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}
