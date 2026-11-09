use std::path::{Path, PathBuf};

use crate::persist::{Config, PersistenceState};
use crate::size::{FolderSizeIndexStorage, SizeService};
#[cfg(feature = "test-utils")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "test-utils")]
use std::time::{Duration, Instant};

use crate::ipc::types::{
    path_strings, AddIndexExclusionRequest, AddIndexExclusionResponse, CancelSizeRequest,
    CancelSizeResponse, CancelSizesRequest, CancelSizesResponse, FolderSizeRequest,
    FolderSizesRequest, IndexCommandError, IndexExclusionsResponse, PickIndexExclusionsResponse,
    RemoveIndexExclusionRequest, SizeStateEvent, SizeSubscriptionsRequest,
};
use tauri::State;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter};
#[cfg(not(feature = "test-utils"))]
use tauri_plugin_dialog::DialogExt;

#[cfg(not(feature = "test-utils"))]
const EXCLUSION_PICKER_TITLE: &str = "Exclude folders";

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
        excluded_roots: update.excluded_roots,
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

#[tauri::command]
pub fn add_index_exclusion(
    payload: AddIndexExclusionRequest,
    sizes: State<'_, SizeService>,
    persistence: State<'_, PersistenceState>,
) -> Result<AddIndexExclusionResponse, IndexCommandError> {
    let candidates: Vec<PathBuf> = payload.paths.iter().map(PathBuf::from).collect();
    let change = sizes.add_index_exclusions(&candidates)?;
    Ok(AddIndexExclusionResponse {
        exclusions: persist_exclusions(&persistence, &change.exclusions),
        outcomes: change.verdicts.into_iter().map(Into::into).collect(),
    })
}

#[tauri::command]
pub fn remove_index_exclusion(
    payload: RemoveIndexExclusionRequest,
    sizes: State<'_, SizeService>,
    persistence: State<'_, PersistenceState>,
) -> Result<IndexExclusionsResponse, IndexCommandError> {
    let exclusions = sizes.remove_index_exclusion(Path::new(&payload.path))?;
    Ok(IndexExclusionsResponse {
        exclusions: persist_exclusions(&persistence, &exclusions),
    })
}

#[tauri::command]
pub fn retry_folder_size_index(sizes: State<'_, SizeService>) -> Result<(), IndexCommandError> {
    sizes.retry_folder_size_index()?;
    Ok(())
}

#[tauri::command]
pub fn rebuild_folder_size_index(sizes: State<'_, SizeService>) -> Result<(), IndexCommandError> {
    sizes.rebuild_folder_size_index()?;
    Ok(())
}

#[tauri::command(async)]
pub fn folder_size_index_storage(
    sizes: State<'_, SizeService>,
) -> Result<FolderSizeIndexStorage, IndexCommandError> {
    Ok(sizes.folder_size_index_storage()?)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command(async)]
pub fn pick_index_exclusions(
    app: AppHandle,
    sizes: State<'_, SizeService>,
) -> Result<PickIndexExclusionsResponse, IndexCommandError> {
    sizes.require_index()?;
    let picked = app
        .dialog()
        .file()
        .set_title(EXCLUSION_PICKER_TITLE)
        .blocking_pick_folders()
        .unwrap_or_default();
    let paths: Vec<PathBuf> = picked
        .into_iter()
        .filter_map(|folder| folder.into_path().ok())
        .collect();
    Ok(PickIndexExclusionsResponse {
        paths: path_strings(&paths),
    })
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn pick_index_exclusions(
    sizes: State<'_, SizeService>,
) -> Result<PickIndexExclusionsResponse, IndexCommandError> {
    sizes.require_index()?;
    Ok(PickIndexExclusionsResponse { paths: Vec::new() })
}

fn persist_exclusions(persistence: &PersistenceState, exclusions: &[PathBuf]) -> Vec<String> {
    let index_exclusions = path_strings(exclusions);
    persistence.config.replace(Config {
        index_exclusions: index_exclusions.clone(),
        ..persistence.config.current()
    });
    index_exclusions
}
