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

/// LaunchServices may keep its OS-owned confirmation UI open for longer than
/// the normal short IPC deadline. The macOS adapter still has its own
/// completion safeguard, so this is a margin for scheduling/serialization
/// rather than unbounded work.
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

/// Background, cache-only warming of the native context-menu cache for a
/// batch of representative single-item requests. Fire-and-forget: enumerates
/// on a dedicated warm pool/executor so a live right-click is never delayed,
/// never touches the invoke-token store, and emits no events.
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

/// Synchronous, `test-utils`-only variant of [`warm_native_menus`]: warms
/// each request through the same shared handle the production command uses
/// (exercising the handle's constructor/`Clone`/`warm` in the covered
/// coverage build), against the in-memory fake provider, and returns the
/// newly-inserted cache keys for assertions.
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

/// LaunchServices has an asynchronous completion callback and may involve a
/// system confirmation dialog. The named app-picker executor owns admission,
/// caller deadline, and cooperative cancellation; the platform request itself
/// is deliberately not force-killed once issued.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn set_default_application(
    payload: SetDefaultApplicationRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<MenuActionStatus, String> {
    executor
        .latency_async_cancellable_with_deadline(
            "app-picker-launch-services".into(),
            crate::ipc::executor::Cancellation::default(),
            SET_DEFAULT_APPLICATION_DEADLINE,
            move |cancellation| async move {
                Ok(app_picker::set_default_application_cancellable(payload, cancellation).await)
            },
        )
        .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub async fn set_default_application(payload: SetDefaultApplicationRequest) -> MenuActionStatus {
    // The deterministic fake has no platform work to enqueue.
    app_picker::set_default_application(payload).await
}

#[tauri::command]
pub fn get_default_application(
    payload: GetDefaultApplicationRequest,
) -> GetDefaultApplicationResponse {
    app_picker::get_default_application(payload)
}
