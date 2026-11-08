use crate::ipc::icon_batch::IconBatcher;
#[cfg(not(feature = "test-utils"))]
use rayon::prelude::*;
#[cfg(not(feature = "test-utils"))]
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::ipc::types::{IconStateEvent, RequestIconsRequest};
#[cfg(not(feature = "test-utils"))]
use std::path::Path;
#[cfg(not(feature = "test-utils"))]
use tauri::State;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter};

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_icons(
    payload: RequestIconsRequest,
    app: AppHandle,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) {
    resolve_and_emit_icons(payload.paths, app, Arc::clone(executor.inner()));
}

#[cfg(not(feature = "test-utils"))]
fn resolve_and_emit_icons(
    paths: Vec<String>,
    app: AppHandle,
    executor: Arc<crate::ipc::executor::IpcExecutor>,
) {
    let Some(pool) = crate::file_icons::icon_pool() else {
        emit_icons_on_executor(paths, app, executor);
        return;
    };
    pool.spawn(move || emit_icons_in_parallel(paths, &app));
}

#[cfg(not(feature = "test-utils"))]
fn emit_icons_on_executor(
    paths: Vec<String>,
    app: AppHandle,
    executor: Arc<crate::ipc::executor::IpcExecutor>,
) {
    tauri::async_runtime::spawn(async move {
        let _ = executor
            .latency("icons".into(), move || {
                emit_icons(paths, &app);
                Ok(())
            })
            .await;
    });
}

#[cfg(not(feature = "test-utils"))]
fn resolved_icon_event(path: String) -> IconStateEvent {
    let icon_data_url = crate::file_icons::resolve_icon(Path::new(&path), false);
    IconStateEvent {
        path,
        icon_data_url,
    }
}

#[cfg(not(feature = "test-utils"))]
fn emit_icon_batch(app: &AppHandle, batch: Option<Vec<IconStateEvent>>) {
    if let Some(batch) = batch {
        let _ = app.emit(crate::ipc::events::ICON_STATE, batch);
    }
}

#[cfg(not(feature = "test-utils"))]
fn emit_icons_in_parallel(paths: Vec<String>, app: &AppHandle) {
    let batcher = Mutex::new(IconBatcher::new(Instant::now()));
    paths.into_par_iter().for_each(|path| {
        let event = resolved_icon_event(path);
        let flushed = batcher
            .lock()
            .expect("icon batcher lock")
            .push(event, Instant::now());
        emit_icon_batch(app, flushed);
    });
    let remainder = batcher
        .into_inner()
        .expect("icon batcher lock")
        .drain_remainder();
    emit_icon_batch(app, remainder);
}

#[cfg(not(feature = "test-utils"))]
fn emit_icons(paths: Vec<String>, app: &AppHandle) {
    let mut batcher = IconBatcher::new(Instant::now());
    for path in paths {
        let flushed = batcher.push(resolved_icon_event(path), Instant::now());
        emit_icon_batch(app, flushed);
    }
    emit_icon_batch(app, batcher.drain_remainder());
}

/// `test-utils` synchronous variant: returns the same batched shape the
/// production command emits over IPC (`Vec<Vec<IconStateEvent>>`, one inner
/// `Vec` per flushed batch) instead of a single flat list, so tests can
/// assert on the number of batches produced.
#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_icons(payload: RequestIconsRequest) -> Vec<Vec<IconStateEvent>> {
    fake_icon_batches(payload.paths)
}

#[cfg(feature = "test-utils")]
fn fake_icon_batches(paths: Vec<String>) -> Vec<Vec<IconStateEvent>> {
    let mut batcher = IconBatcher::new(Instant::now());
    let unresolved = paths.into_iter().map(|path| IconStateEvent {
        path,
        icon_data_url: None,
    });
    let mut batches: Vec<_> = unresolved
        .filter_map(|event| batcher.push(event, Instant::now()))
        .collect();
    batches.extend(batcher.drain_remainder());
    batches
}
