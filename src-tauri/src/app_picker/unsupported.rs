use super::types::{GetDefaultApplicationResponse, ListApplicationsResponse, MacApp};
use crate::ipc::types::MenuActionStatus;

/// Under `feature = "test-utils"` the app list is a small, deterministic fake
/// so IPC-contract tests have a non-empty, stable response without ever
/// scanning the real `/Applications` folder.
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
#[cfg(not(feature = "test-utils"))]
pub fn list_applications() -> ListApplicationsResponse {
    ListApplicationsResponse { apps: Vec::new() }
}

pub fn set_default_application() -> MenuActionStatus {
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
