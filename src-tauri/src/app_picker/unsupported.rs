#[cfg(feature = "test-utils")]
use super::types::MacApp;
use super::types::{GetDefaultApplicationResponse, ListApplicationsResponse};
use crate::ipc::types::MenuActionStatus;

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

#[cfg(not(feature = "test-utils"))]
pub fn list_applications() -> ListApplicationsResponse {
    ListApplicationsResponse { apps: Vec::new() }
}

pub async fn set_default_application() -> MenuActionStatus {
    MenuActionStatus::unsupported("unsupported")
}

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
