use crate::fs;
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem_unless_cancelled;
use super::logged_error;
use crate::fs::DirectoryEntry;
use crate::ipc::types::{CreateEntryRequest, RenameEntryRequest};
#[cfg(not(feature = "test-utils"))]
use tauri::State;

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn create_folder(
    payload: CreateEntryRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<DirectoryEntry, String> {
    let key = payload.parent.clone();
    latency_filesystem_unless_cancelled(&executor, key, "create folder cancelled", move || {
        create_folder_impl(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn create_folder(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    create_folder_impl(payload)
}

fn create_folder_impl(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    fs::create_directory(&payload.parent, &payload.name).map_err(|error| {
        logged_error(format!(
            "Failed to create folder \"{}\": {error}",
            payload.name
        ))
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn create_file(
    payload: CreateEntryRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<DirectoryEntry, String> {
    let key = payload.parent.clone();
    latency_filesystem_unless_cancelled(&executor, key, "create file cancelled", move || {
        create_file_impl(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn create_file(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    create_file_impl(payload)
}

fn create_file_impl(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    fs::create_file(&payload.parent, &payload.name).map_err(|error| {
        logged_error(format!(
            "Failed to create file \"{}\": {error}",
            payload.name
        ))
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn rename_entry(
    payload: RenameEntryRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<DirectoryEntry, String> {
    let key = payload.path.clone();
    latency_filesystem_unless_cancelled(&executor, key, "rename cancelled", move || {
        rename_entry_impl(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn rename_entry(payload: RenameEntryRequest) -> Result<DirectoryEntry, String> {
    rename_entry_impl(payload)
}

fn rename_entry_impl(payload: RenameEntryRequest) -> Result<DirectoryEntry, String> {
    fs::rename_entry(&payload.path, &payload.new_name)
        .map_err(|error| logged_error(format!("Failed to rename \"{}\": {error}", payload.path)))
}
