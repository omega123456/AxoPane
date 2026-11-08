#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;

use super::logged_error;
#[cfg(not(feature = "test-utils"))]
use super::{latency_filesystem, latency_filesystem_cancellable};
use crate::ipc::types::{
    DeleteFromTrashRequest, ListTrashResponse, RestoreTrashRequest, TrashEntriesRequest,
};
#[cfg(not(feature = "test-utils"))]
use tauri::State;

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn move_to_trash(
    payload: TrashEntriesRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    let key = payload
        .paths
        .first()
        .cloned()
        .unwrap_or_else(|| "trash".into());
    latency_filesystem_cancellable(&executor, key, move |cancel| {
        move_to_trash_impl_cancellable(payload, cancel)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn move_to_trash(payload: TrashEntriesRequest) -> Result<(), String> {
    move_to_trash_impl(payload)
}

#[cfg(feature = "test-utils")]
fn move_to_trash_impl(payload: TrashEntriesRequest) -> Result<(), String> {
    crate::trash::move_to_trash(&payload.paths)
        .map_err(|error| logged_error(format!("Failed to move items to trash: {error}")))
}

#[cfg(not(feature = "test-utils"))]
fn move_to_trash_impl_cancellable(
    payload: TrashEntriesRequest,
    cancellation: &crate::ipc::executor::Cancellation,
) -> Result<(), String> {
    crate::trash::move_to_trash_cancellable(&payload.paths, || cancellation.is_cancelled()).map_err(
        |error| {
            let message = format!("Failed to move items to trash: {error}");
            log::error!("{message}");
            message
        },
    )
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn list_trash(
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ListTrashResponse, String> {
    latency_filesystem(&executor, "trash".into(), list_trash_impl).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_trash() -> Result<ListTrashResponse, String> {
    list_trash_impl()
}

fn list_trash_impl() -> Result<ListTrashResponse, String> {
    crate::trash::list_trash()
        .map(|entries| ListTrashResponse { entries })
        .map_err(|error| {
            let message = format!("Failed to list trash: {error}");
            log::error!("{message}");
            message
        })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn restore_from_trash(
    payload: RestoreTrashRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    latency_filesystem(&executor, "trash".into(), move || {
        restore_from_trash_impl(payload)
    })
    .await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn restore_from_trash(payload: RestoreTrashRequest) -> Result<(), String> {
    restore_from_trash_impl(payload)
}

fn restore_from_trash_impl(payload: RestoreTrashRequest) -> Result<(), String> {
    crate::trash::restore_from_trash(&payload.ids)
        .map_err(|error| logged_error(format!("Failed to restore items from trash: {error}")))
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn empty_trash(
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    latency_filesystem(&executor, "trash".into(), empty_trash_impl).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn empty_trash() -> Result<(), String> {
    empty_trash_impl()
}

fn empty_trash_impl() -> Result<(), String> {
    crate::trash::empty_trash()
        .map_err(|error| logged_error(format!("Failed to empty trash: {error}")))
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn delete_from_trash(
    payload: DeleteFromTrashRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    latency_filesystem(&executor, "trash".into(), move || {
        delete_from_trash_impl(payload)
    })
    .await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn delete_from_trash(payload: DeleteFromTrashRequest) -> Result<(), String> {
    delete_from_trash_impl(payload)
}

fn delete_from_trash_impl(payload: DeleteFromTrashRequest) -> Result<(), String> {
    crate::trash::delete_from_trash(&payload.ids).map_err(|error| {
        logged_error(format!(
            "Failed to permanently delete items from trash: {error}"
        ))
    })
}
