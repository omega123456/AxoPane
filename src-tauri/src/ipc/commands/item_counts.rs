use crate::item_counts::ItemCountService;
use std::sync::Arc;

use crate::ipc::types::{
    ActiveItemsSortRequest, ActiveItemsSortResponse, VisibleItemCountsRequest,
};
#[cfg(feature = "test-utils")]
use tauri::State;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter, Manager};

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_visible_item_counts(
    payload: VisibleItemCountsRequest,
    app: AppHandle,
    services: crate::ipc::command_services::ItemCountServices<'_>,
) {
    let generation = services
        .sessions
        .watch_revision_for_pane_path(&payload.context.pane_id, &payload.context.path)
        .unwrap_or(payload.context.request_id);
    let item_counts = &services.item_counts;
    let plan = item_counts.plan_automatic_request_with_generation(&payload, generation);
    if plan.is_empty() {
        return;
    }

    if !item_counts.enqueue_automatic_request(plan) {
        return;
    }
    let executor = Arc::clone(services.executor.inner());
    tauri::async_runtime::spawn(async move {
        let _ = executor
            .latency("item-counts".into(), move || {
                drain_automatic_item_counts(&app);
                Ok(())
            })
            .await;
    });
}

#[cfg(not(feature = "test-utils"))]
fn drain_automatic_item_counts(app: &AppHandle) {
    let service = app.state::<ItemCountService>();
    service.process_automatic_queue(|event| {
        let _ = app.emit(crate::ipc::events::ITEM_COUNT, event);
    });
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_visible_item_counts(
    payload: VisibleItemCountsRequest,
    state: State<'_, ItemCountService>,
    session_state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) {
    let generation = session_state
        .watch_revision_for_pane_path(&payload.context.pane_id, &payload.context.path)
        .unwrap_or(payload.context.request_id);
    let plan = state.plan_automatic_request_with_generation(&payload, generation);
    state.process_automatic_request(plan, |event| state.record_test_event(event));
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn sort_active_items(
    payload: ActiveItemsSortRequest,
    services: crate::ipc::command_services::ItemCountServices<'_>,
) -> Result<ActiveItemsSortResponse, String> {
    let key = payload.context.path.clone();
    let item_counts = services.item_counts.inner().clone();
    let sessions = Arc::clone(services.sessions.inner());
    services
        .executor
        .latency(key, move || {
            item_counts
                .sort_active_items_with_session(&payload, Some(&sessions))
                .map_err(|error| {
                    let message = format!(
                        "Failed to sort \"{}\" by Items: {error}",
                        payload.context.path
                    );
                    log::error!("{message}");
                    message
                })
        })
        .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn sort_active_items(
    payload: ActiveItemsSortRequest,
    state: State<'_, ItemCountService>,
    session_state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> Result<ActiveItemsSortResponse, String> {
    state
        .sort_active_items_with_session(&payload, Some(session_state.inner()))
        .map_err(|error| {
            let message = format!(
                "Failed to sort \"{}\" by Items: {error}",
                payload.context.path
            );
            log::error!("{message}");
            message
        })
}
