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

pub fn list_applications() -> ListApplicationsResponse {
    #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
    {
        macos::list_applications()
    }

    #[cfg(any(feature = "test-utils", not(target_os = "macos")))]
    {
        unsupported::list_applications()
    }
}

pub async fn set_default_application(request: SetDefaultApplicationRequest) -> MenuActionStatus {
    set_default_application_cancellable(request, crate::ipc::executor::Cancellation::default())
        .await
}

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
        result
    }

    #[cfg(any(feature = "test-utils", not(target_os = "macos")))]
    {
        let _ = request;
        let _ = cancellation;
        unsupported::set_default_application().await
    }
}

pub fn get_default_application(
    request: GetDefaultApplicationRequest,
) -> GetDefaultApplicationResponse {
    #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
    {
        GetDefaultApplicationResponse {
            app: macos::get_default_application(&request),
        }
    }

    #[cfg(any(feature = "test-utils", not(target_os = "macos")))]
    {
        let _ = request;
        unsupported::get_default_application()
    }
}
