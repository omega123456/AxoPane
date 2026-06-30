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
pub fn set_default_application(request: SetDefaultApplicationRequest) -> MenuActionStatus {
    #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
    {
        return macos::set_default_application(&request);
    }

    #[cfg(any(feature = "test-utils", not(target_os = "macos")))]
    {
        let _ = request;
        unsupported::set_default_application()
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
