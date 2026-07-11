#[cfg(feature = "test-utils")]
use super::types::MacApp;
use super::types::{GetDefaultApplicationResponse, ListApplicationsResponse};
use crate::ipc::types::MenuActionStatus;

/// Under `feature = "test-utils"` the app list is a small, deterministic fake
/// so IPC-contract tests have a non-empty, stable response without ever
/// scanning the real `/Applications` folder.
///
/// `async` to keep an identical signature to `macos::list_applications`
/// across both `cfg`-gated implementations `app_picker::list_applications`
/// dispatches between. No actual asynchronous work happens here - this is a
/// fixed fake - so this resolves immediately.
#[cfg(feature = "test-utils")]
pub fn list_applications() -> ListApplicationsResponse {
    ListApplicationsResponse {
        apps: vec![MacApp {
            name: "Fake Preview".to_string(),
            bundle_path: "/Applications/Fake Preview.app".to_string(),
            bundle_id: Some("com.example.fake-preview".to_string()),
            icon_data_url: None,
        }],
    }
}

/// On real, non-macOS builds (e.g. Windows) there is no app picker at all.
///
/// Kept synchronous to match the signature `app_picker::list_applications`
/// dispatches to across both `cfg`-gated implementations - Windows has no
/// real enumeration to perform, so this resolves immediately.
#[cfg(not(feature = "test-utils"))]
pub fn list_applications() -> ListApplicationsResponse {
    ListApplicationsResponse { apps: Vec::new() }
}

/// `async` to keep an identical signature to `macos::set_default_application`
/// across both `cfg`-gated implementations `app_picker::set_default_application`
/// dispatches between. No actual asynchronous work happens here - Windows and
/// `test-utils` builds have no real write to perform - so this resolves
/// immediately.
pub async fn set_default_application() -> MenuActionStatus {
    MenuActionStatus::unsupported("unsupported")
}

/// Under `feature = "test-utils"` the default app is a small, deterministic
/// fake so the Properties dialog has something stable to render in tests.
#[cfg(feature = "test-utils")]
pub fn get_default_application() -> GetDefaultApplicationResponse {
    GetDefaultApplicationResponse {
        app: Some(MacApp {
            name: "Fake Preview".to_string(),
            bundle_path: "/Applications/Fake Preview.app".to_string(),
            bundle_id: Some("com.example.fake-preview".to_string()),
            icon_data_url: None,
        }),
    }
}

#[cfg(not(feature = "test-utils"))]
pub fn get_default_application() -> GetDefaultApplicationResponse {
    GetDefaultApplicationResponse { app: None }
}
