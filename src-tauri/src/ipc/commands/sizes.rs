use crate::size::SizeService;
#[cfg(feature = "test-utils")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "test-utils")]
use std::time::{Duration, Instant};

use crate::ipc::types::{
    CancelSizeRequest, CancelSizeResponse, CancelSizesRequest, CancelSizesResponse,
    FolderSizeRequest, FolderSizesRequest, SizeStateEvent, SizeSubscriptionsRequest,
};
use tauri::State;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter};

#[cfg(feature = "test-utils")]
const SIZE_UPDATE_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[tauri::command]
pub fn set_size_subscriptions(payload: SizeSubscriptionsRequest, state: State<'_, SizeService>) {
    state.set_subscriptions(crate::size::PaneSubscriptions {
        pane_id: payload.pane_id,
        current_path: payload.current_path,
        folders: payload.folder_paths,
    });
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_folder_size(
    payload: FolderSizeRequest,
    app: AppHandle,
    state: State<'_, SizeService>,
) {
    request_folder_sizes(
        FolderSizesRequest {
            paths: vec![payload.path],
            mode: payload.mode,
        },
        app,
        state,
    );
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_folder_size(
    payload: FolderSizeRequest,
    state: State<'_, SizeService>,
) -> Vec<Vec<SizeStateEvent>> {
    request_folder_sizes(
        FolderSizesRequest {
            paths: vec![payload.path],
            mode: payload.mode,
        },
        state,
    )
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_folder_sizes(
    payload: FolderSizesRequest,
    app: AppHandle,
    state: State<'_, SizeService>,
) {
    let app_handle = app.clone();
    let request = crate::size::SizeRequest {
        paths: payload.paths,
        mode: payload.mode.unwrap_or_default(),
    };
    state.request_paths(request, move |updates| {
        let batch = updates
            .into_iter()
            .map(size_state_event_from_update)
            .collect::<Vec<_>>();
        if !batch.is_empty() {
            let _ = app_handle.emit(crate::ipc::events::SIZE_STATE, batch);
        }
    });
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_folder_sizes(
    payload: FolderSizesRequest,
    state: State<'_, SizeService>,
) -> Vec<Vec<SizeStateEvent>> {
    let expected_paths = payload.paths.len();
    if expected_paths == 0 {
        return Vec::new();
    }

    let updates = Arc::new(Mutex::new(Vec::<Vec<SizeStateEvent>>::new()));
    let updates_for_emitter = Arc::clone(&updates);

    let request = crate::size::SizeRequest {
        paths: payload.paths,
        mode: payload.mode.unwrap_or_default(),
    };
    state.request_paths(request, move |batch| {
        let events = batch
            .into_iter()
            .map(size_state_event_from_update)
            .collect::<Vec<_>>();
        if !events.is_empty() {
            updates_for_emitter
                .lock()
                .expect("size updates lock")
                .push(events);
        }
    });

    let deadline = Instant::now() + Duration::from_secs(1);
    let before_deadline = || Instant::now() < deadline;
    while before_deadline() {
        let recorded = updates.lock().expect("size updates lock");
        let terminal_count = recorded
            .iter()
            .flatten()
            .filter(|event| {
                matches!(
                    event.state,
                    crate::size::SizeStateKind::Ready
                        | crate::size::SizeStateKind::Error
                        | crate::size::SizeStateKind::Na
                        | crate::size::SizeStateKind::Unknown
                )
            })
            .count();
        if terminal_count >= expected_paths {
            return recorded.clone();
        }
        drop(recorded);
        std::thread::sleep(SIZE_UPDATE_POLL_INTERVAL);
    }

    let recorded = updates.lock().expect("size updates lock").clone();
    recorded
}

pub fn size_state_event_from_update(update: crate::size::SizeUpdate) -> SizeStateEvent {
    SizeStateEvent {
        path: update.path,
        state: update.state,
        source: update.source,
        size_bytes: update.size_bytes,
        na_reason: update.na_reason,
        is_lower_bound: update.is_lower_bound,
        skipped_roots: update.skipped_roots,
        skipped_more: update.skipped_more,
    }
}

#[tauri::command]
pub fn cancel_size(
    payload: CancelSizeRequest,
    state: State<'_, SizeService>,
) -> CancelSizeResponse {
    CancelSizeResponse {
        cancelled: state.cancel(&payload.path),
    }
}

#[tauri::command]
pub fn cancel_sizes(
    payload: CancelSizesRequest,
    state: State<'_, SizeService>,
) -> CancelSizesResponse {
    CancelSizesResponse {
        cancelled: state.cancel_many(&payload.paths),
    }
}
