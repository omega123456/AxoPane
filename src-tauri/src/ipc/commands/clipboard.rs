#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::{latency_filesystem, latency_filesystem_cancellable};
use crate::ipc::types::{
    FileClipboardItem, FileClipboardMode, ReadFileClipboardResponse, WriteFileClipboardRequest,
};
use std::path::Path;
#[cfg(not(feature = "test-utils"))]
use tauri::State;

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn write_file_clipboard(
    payload: WriteFileClipboardRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    latency_filesystem_cancellable(&executor, "clipboard".into(), move |cancel| {
        if cancel.is_cancelled() {
            return Err("clipboard write cancelled".to_string());
        }
        write_file_clipboard_impl(payload)
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn write_file_clipboard(payload: WriteFileClipboardRequest) -> Result<(), String> {
    write_file_clipboard_impl(payload)
}

fn write_file_clipboard_impl(payload: WriteFileClipboardRequest) -> Result<(), String> {
    crate::clipboard::write_paths(
        match payload.mode {
            FileClipboardMode::Copy => crate::clipboard::ClipboardMode::Copy,
            FileClipboardMode::Move => crate::clipboard::ClipboardMode::Move,
        },
        &payload.paths,
    )
    .map_err(|error| {
        let message = format!("Failed to update OS clipboard: {error}");
        log::warn!("{message}");
        message
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn read_file_clipboard(
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ReadFileClipboardResponse, String> {
    latency_filesystem(&executor, "clipboard".into(), read_file_clipboard_impl).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn read_file_clipboard() -> Result<ReadFileClipboardResponse, String> {
    read_file_clipboard_impl()
}

fn read_file_clipboard_impl() -> Result<ReadFileClipboardResponse, String> {
    let clipboard_files = crate::clipboard::read_paths().map_err(|error| {
        let message = format!("Failed to read OS clipboard: {error}");
        log::warn!("{message}");
        message
    })?;

    Ok(ReadFileClipboardResponse {
        mode: match clipboard_files.mode {
            crate::clipboard::ClipboardMode::Copy => FileClipboardMode::Copy,
            crate::clipboard::ClipboardMode::Move => FileClipboardMode::Move,
        },
        items: file_clipboard_items(clipboard_files.paths),
    })
}

pub fn file_clipboard_items(paths: Vec<String>) -> Vec<FileClipboardItem> {
    paths
        .into_iter()
        .map(|path| {
            let name = Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            FileClipboardItem { path, name }
        })
        .collect()
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn clear_file_clipboard(
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    latency_filesystem(&executor, "clipboard".into(), clear_file_clipboard_impl).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn clear_file_clipboard() -> Result<(), String> {
    clear_file_clipboard_impl()
}

fn clear_file_clipboard_impl() -> Result<(), String> {
    crate::clipboard::clear().map_err(|error| {
        let message = format!("Failed to clear OS clipboard: {error}");
        log::warn!("{message}");
        message
    })
}
