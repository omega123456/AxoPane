use crate::app_picker;
use crate::native_menu::NativeMenuService;
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;
#[cfg(not(feature = "test-utils"))]
use std::time::Duration;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem;
#[cfg(feature = "test-utils")]
use crate::ipc::types::WarmNativeMenusResponse;
use crate::ipc::types::{
    GetDefaultApplicationRequest, GetDefaultApplicationResponse, InvokeNativeMenuRequest,
    ListApplicationsResponse, LoadNativeMenuRequest, LoadNativeMenuResponse, MenuActionStatus,
    OpenWithRequest, SetDefaultApplicationRequest, ShowPropertiesRequest, WarmNativeMenusRequest,
};
use tauri::State;

#[cfg(not(feature = "test-utils"))]
const SET_DEFAULT_APPLICATION_DEADLINE: Duration = Duration::from_secs(15);

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn load_native_menu(
    payload: LoadNativeMenuRequest,
    state: State<'_, Arc<NativeMenuService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<LoadNativeMenuResponse, String> {
    let service = Arc::clone(state.inner());
    latency_filesystem(&executor, "native-menu".into(), move || {
        service.load_menu(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn load_native_menu(
    payload: LoadNativeMenuRequest,
    state: State<'_, NativeMenuService>,
) -> LoadNativeMenuResponse {
    state.load_menu(payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn invoke_native_menu_action(
    payload: InvokeNativeMenuRequest,
    state: State<'_, Arc<NativeMenuService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<MenuActionStatus, String> {
    let service = Arc::clone(state.inner());
    latency_filesystem(&executor, "native-menu".into(), move || {
        service.invoke_menu_action(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn invoke_native_menu_action(
    payload: InvokeNativeMenuRequest,
    state: State<'_, NativeMenuService>,
) -> MenuActionStatus {
    state.invoke_menu_action(payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn show_properties(
    payload: ShowPropertiesRequest,
    state: State<'_, Arc<NativeMenuService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<MenuActionStatus, String> {
    let service = Arc::clone(state.inner());
    latency_filesystem(&executor, "native-menu".into(), move || {
        service.show_properties(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn show_properties(
    payload: ShowPropertiesRequest,
    state: State<'_, NativeMenuService>,
) -> MenuActionStatus {
    state.show_properties(payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn open_with(
    payload: OpenWithRequest,
    state: State<'_, Arc<NativeMenuService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<MenuActionStatus, String> {
    let service = Arc::clone(state.inner());
    latency_filesystem(&executor, "native-menu".into(), move || {
        service.open_with(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn open_with(
    payload: OpenWithRequest,
    state: State<'_, NativeMenuService>,
) -> MenuActionStatus {
    state.open_with(payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn warm_native_menus(
    payload: WarmNativeMenusRequest,
    state: State<'_, Arc<NativeMenuService>>,
) {
    log::debug!(
        "received native menu warm batch with {} requests",
        payload.requests.len()
    );
    let Some(pool) = crate::native_menu::warm_pool::warm_pool() else {
        log::debug!("skipping native menu warm batch because the warm pool is unavailable");
        return;
    };

    let handle = state.warm_handle();
    pool.spawn(move || {
        for request in &payload.requests {
            handle.warm(request);
        }
        log::debug!(
            "finished native menu warm batch with {} requests",
            payload.requests.len()
        );
    });
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn warm_native_menus(
    payload: WarmNativeMenusRequest,
    state: State<'_, NativeMenuService>,
) -> WarmNativeMenusResponse {
    log::debug!(
        "received native menu warm batch with {} requests",
        payload.requests.len()
    );
    let handle = state.warm_handle();
    let warmed_keys: Vec<String> = payload
        .requests
        .iter()
        .filter_map(|request| handle.clone().warm(request))
        .collect();

    log::debug!(
        "finished native menu warm batch; inserted {} new cache keys",
        warmed_keys.len()
    );
    WarmNativeMenusResponse { warmed_keys }
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn list_applications(
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ListApplicationsResponse, String> {
    latency_filesystem(
        &executor,
        "app-picker".into(),
        app_picker::list_applications,
    )
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub async fn list_applications() -> ListApplicationsResponse {
    app_picker::list_applications()
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn set_default_application(
    payload: SetDefaultApplicationRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<MenuActionStatus, String> {
    executor
        .latency_async_cancellable(
            crate::ipc::executor::LatencyRequest::new("app-picker-launch-services".into())
                .with_deadline(SET_DEFAULT_APPLICATION_DEADLINE),
            move |cancellation| async move {
                Ok(app_picker::set_default_application_cancellable(payload, cancellation).await)
            },
        )
        .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub async fn set_default_application(payload: SetDefaultApplicationRequest) -> MenuActionStatus {
    app_picker::set_default_application(payload).await
}

#[tauri::command]
pub fn get_default_application(
    payload: GetDefaultApplicationRequest,
) -> GetDefaultApplicationResponse {
    app_picker::get_default_application(payload)
}
