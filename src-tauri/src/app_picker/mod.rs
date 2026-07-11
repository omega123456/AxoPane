pub mod bundle;
pub mod icns;
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub mod macos;
pub mod scan;
pub mod types;
#[cfg(any(feature = "test-utils", not(target_os = "macos")))]
pub mod unsupported;

use crate::ipc::types::MenuActionStatus;
use types::{
    GetDefaultApplicationRequest, GetDefaultApplicationResponse, ListApplicationsResponse,
    SetDefaultApplicationRequest,
};

/// Lists candidate apps for the "Set Default Application…" picker. Real
/// enumeration only ever runs on macOS outside test builds; every other
/// configuration (Windows, or any target under `feature = "test-utils"`)
/// resolves to a safe stub so tests never depend on the real `/Applications`
/// folder and Windows never gains this macOS-only surface.
///
/// `async` so the genuinely-async `list_applications` Tauri command can await
/// the macOS implementation without ever blocking the calling
/// (potentially main/UI) thread - see `macos::list_applications` for how the
/// underlying blocking enumeration/icon-resolution work is dispatched
/// off-thread.
pub fn list_applications() -> ListApplicationsResponse {
    #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
    {
        return macos::list_applications();
    }

    #[cfg(any(feature = "test-utils", not(target_os = "macos")))]
    {
        unsupported::list_applications()
    }
}

/// Permanently associates the file's extension/type with the chosen app via
/// LaunchServices. Real writes only ever happen on macOS outside test
/// builds, so this never touches a real machine-global LaunchServices
/// database during tests, and never runs at all on Windows.
///
/// `async` so the genuinely-async `set_default_application` Tauri command can
/// await the macOS implementation without ever blocking the calling
/// (potentially main/UI) thread - see `macos::set_default_application` for
/// how the underlying blocking `NSWorkspace` wait is dispatched off-thread.
pub async fn set_default_application(request: SetDefaultApplicationRequest) -> MenuActionStatus {
    set_default_application_cancellable(request, crate::ipc::executor::Cancellation::default())
        .await
}

/// App-picker execution contract used by the named IPC app-picker owner.
/// Cancellation is cooperative: it prevents a request from entering
/// LaunchServices and prevents a late completion from being returned. An
/// already-issued LaunchServices request is OS-owned and cannot safely be
/// force-killed, so it may complete after the IPC caller's deadline.
pub async fn set_default_application_cancellable(
    request: SetDefaultApplicationRequest,
    cancellation: crate::ipc::executor::Cancellation,
) -> MenuActionStatus {
    if cancellation.is_cancelled() {
        return MenuActionStatus::unsupported("default-application-cancelled");
    }
    #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
    {
        let result = macos::set_default_application(&request).await;
        if cancellation.is_cancelled() {
            return MenuActionStatus::unsupported("default-application-cancelled");
        }
        return result;
    }

    #[cfg(any(feature = "test-utils", not(target_os = "macos")))]
    {
        let _ = request;
        let _ = cancellation;
        unsupported::set_default_application().await
    }
}

/// Looks up the app currently registered as the default handler for the
/// file's type, shown read-only in the Properties dialog. Real lookups only
/// ever happen on macOS outside test builds, mirroring `set_default_application`.
pub fn get_default_application(
    request: GetDefaultApplicationRequest,
) -> GetDefaultApplicationResponse {
    #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
    {
        return GetDefaultApplicationResponse {
            app: macos::get_default_application(&request),
        };
    }

    #[cfg(any(feature = "test-utils", not(target_os = "macos")))]
    {
        let _ = request;
        unsupported::get_default_application()
    }
}
