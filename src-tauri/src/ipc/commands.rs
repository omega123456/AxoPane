use crate::app_picker;
use crate::fs;
use crate::ipc::icon_batch::IconBatcher;
use crate::item_counts::ItemCountService;
use crate::native_menu::NativeMenuService;
use crate::ops::{OpSnapshot, OpsService};
use crate::persist::{PersistedStore, PersistenceState};
use crate::size::SizeService;
use crate::volumes;
use crate::watch::WatchService;
#[cfg(not(feature = "test-utils"))]
use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(feature = "test-utils")]
use std::time::Duration;

use super::mock;
#[cfg(feature = "test-utils")]
use super::types::WarmNativeMenusResponse;
use super::types::{
    ActiveItemsSortRequest, ActiveItemsSortResponse, AppConfig, BeginNavigationRequest,
    BeginNavigationResponse, CancelSizeRequest, CancelSizeResponse, CancelSizesRequest,
    CancelSizesResponse, CancelThumbnailsRequest, CreateEntryRequest, DeleteFromTrashRequest,
    EjectVolumeRequest, FileClipboardMode, FolderSizeRequest, FolderSizesRequest,
    GetDefaultApplicationRequest, GetDefaultApplicationResponse, GetSessionRangeRequest,
    IconStateEvent, InitialShellResponse, InvokeNativeMenuRequest, ListApplicationsResponse,
    ListDirRequest, ListDirResponse, ListTrashResponse, ListTreeChildrenRequest,
    ListTreeChildrenResponse, LoadNativeMenuRequest, LoadNativeMenuResponse, LogFrontendRequest,
    MenuActionStatus, OpIdRequest, OpenPathRequest, OpenWithRequest, ReleaseSessionRequest,
    ReleaseSessionResponse, RenameEntryRequest, ReorderOpsRequest, RequestIconsRequest,
    RequestThumbnailsRequest, ResolveConflictRequest, RestoreTrashRequest,
    ReviseSessionViewRequest, SaveConfigRequest, SaveSessionRequest, SessionRangeResponse,
    SessionRejection, SessionState, SetDefaultApplicationRequest, SetLogLevelRequest,
    SetTabWatchRequest, ShowPropertiesRequest, SizeStateEvent, StartOpRequest, TrashEntriesRequest,
    VisibleItemCountsRequest, VolumeInfo, WarmNativeMenusRequest, WriteFileClipboardRequest,
};
use crate::fs::DirectoryEntry;
use std::path::Path;
#[cfg(not(feature = "test-utils"))]
use std::time::Duration;
use tauri::State;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter, Manager};

#[cfg(feature = "test-utils")]
#[inline(never)]
pub fn noop_dir_patch(_: crate::watch::DirPatch) {}

#[cfg(feature = "test-utils")]
#[inline(never)]
pub fn noop_watch_error(_: String, _: String) {}

/// Runs bounded, single-request filesystem work through the owned IPC
/// latency executor. Command handlers only validate/capture payloads before
/// entering this boundary; the executor has a fixed queue/workers, shared
/// coordinator admission, deadline, and deterministic shutdown contract.
///
/// Long-running transfers deliberately do not use this helper; they belong to
/// `OpsService`, which owns cancellation, progress and shutdown instead.
#[cfg(not(feature = "test-utils"))]
async fn latency_filesystem<T: Send + 'static>(
    executor: &crate::ipc::executor::IpcExecutor,
    resource_key: String,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    executor.latency(resource_key, move || Ok(work())).await
}

/// Variant for bounded mutations that can observe cancellation before they
/// touch an OS API and at their own safe internal checkpoints.
#[cfg(not(feature = "test-utils"))]
async fn latency_filesystem_cancellable<T: Send + 'static>(
    executor: &crate::ipc::executor::IpcExecutor,
    resource_key: String,
    work: impl FnOnce(&crate::ipc::executor::Cancellation) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    executor
        .latency_cancellable(
            resource_key,
            crate::ipc::executor::Cancellation::default(),
            work,
        )
        .await
}

#[tauri::command]
pub fn get_initial_shell() -> InitialShellResponse {
    mock::initial_shell()
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_thumbnails(
    payload: RequestThumbnailsRequest,
    state: State<'_, Arc<crate::thumbnails::ThumbnailService>>,
) {
    let subscribers = payload
        .candidates
        .into_iter()
        .map(|candidate| {
            let subscriber = crate::thumbnails::scheduler::ThumbnailSubscriber {
                pane_id: payload.pane_id.clone(),
                tab_id: payload.tab_id.clone(),
                path: payload.path.clone(),
                generation: payload.generation,
            };
            let fingerprint = crate::thumbnails::types::ThumbnailFingerprint::from_metadata(
                Path::new(&candidate.path),
                candidate.modified_unix_seconds,
                candidate.size_bytes,
            );
            (
                subscriber,
                crate::thumbnails::types::ThumbnailCandidate::new(
                    fingerprint,
                    candidate.is_directory,
                ),
            )
        })
        .collect();
    state.request(subscribers);
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_thumbnails(
    payload: RequestThumbnailsRequest,
    state: State<'_, Arc<crate::thumbnails::ThumbnailService>>,
) {
    let subscribers = payload
        .candidates
        .into_iter()
        .map(|candidate| {
            let subscriber = crate::thumbnails::scheduler::ThumbnailSubscriber {
                pane_id: payload.pane_id.clone(),
                tab_id: payload.tab_id.clone(),
                path: payload.path.clone(),
                generation: payload.generation,
            };
            let fingerprint = crate::thumbnails::types::ThumbnailFingerprint::from_metadata(
                Path::new(&candidate.path),
                candidate.modified_unix_seconds,
                candidate.size_bytes,
            );
            (
                subscriber,
                crate::thumbnails::types::ThumbnailCandidate::new(
                    fingerprint,
                    candidate.is_directory,
                ),
            )
        })
        .collect();
    state.request(subscribers);
}

#[tauri::command]
pub fn cancel_thumbnails(
    payload: CancelThumbnailsRequest,
    state: State<'_, Arc<crate::thumbnails::ThumbnailService>>,
) {
    state.cancel(
        &payload.pane_id,
        &payload.tab_id,
        &payload.path,
        payload.generation,
    );
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn list_dir(
    payload: ListDirRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ListDirResponse, String> {
    let key = payload.path.clone();
    executor
        .latency_cancellable(
            key,
            crate::ipc::executor::Cancellation::default(),
            move |cancel| {
                fs::list_dir_with_cancellation(&payload, || cancel.is_cancelled())
                    .map_err(|error| {
                        let message = format!("Failed to load \"{}\": {error}", payload.path);
                        log::error!("{message}");
                        message
                    })
                    .and_then(|outcome| match outcome {
                        fs::ListDirOutcome::Complete(response) => Ok(response),
                        fs::ListDirOutcome::Cancelled => {
                            Err("directory listing cancelled".to_string())
                        }
                    })
            },
        )
        .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_dir(payload: ListDirRequest) -> Result<ListDirResponse, String> {
    list_dir_impl(payload)
}

#[cfg(feature = "test-utils")]
fn list_dir_impl(payload: ListDirRequest) -> Result<ListDirResponse, String> {
    fs::list_dir(&payload).map_err(|error| {
        let message = format!("Failed to load \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

/// Establishes (or replaces) the directory session for a pane.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn begin_directory_session(
    payload: BeginNavigationRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
    registry: State<'_, Arc<volumes::registry::VolumeRegistry>>,
) -> Result<BeginNavigationResponse, String> {
    let resource_key = registry.resolve_resource_key(&payload.path);
    let key = resource_key.clone().unwrap_or_else(|| payload.path.clone());
    let sessions = Arc::clone(state.inner());
    executor
        .latency(key, move || {
            sessions.begin_navigation(payload, None, resource_key)
        })
        .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn begin_directory_session(
    payload: BeginNavigationRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
    coordinator: State<'_, Arc<crate::resource_coordinator::ResourceCoordinator>>,
    registry: State<'_, Arc<volumes::registry::VolumeRegistry>>,
) -> Result<BeginNavigationResponse, String> {
    let resource_key = registry.resolve_resource_key(&payload.path);
    state.begin_navigation(payload, Some(&coordinator), resource_key)
}

/// Idempotent page-index range fetch for an established v2 directory
/// session. Any valid page index may be retried for the same baseline; a
/// stale baseline field is rejected (not served) per Functional Requirement 1.
///
/// The error is the typed [`SessionRejection`] (not a stringified message) so
/// the frontend can branch on `kind` (e.g. "stale, silently re-navigate" vs.
/// "page out of range, a real bug") instead of string-matching.
#[tauri::command]
pub fn get_directory_session_range(
    payload: GetSessionRangeRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> Result<SessionRangeResponse, SessionRejection> {
    state.get_range(&payload)
}

/// Revises the sort/filter/show-hidden/item-count view of an already-active
/// v2 directory session without re-enumerating the filesystem (Phase 4: the
/// frontend calls this instead of re-running `begin_directory_session` for a
/// plain sort/filter change on the same directory). Returns the same shape as
/// `begin_directory_session` (a fresh baseline + first page) so callers can
/// treat "new session" and "revised view" uniformly.
#[tauri::command]
pub fn revise_directory_session_view(
    payload: ReviseSessionViewRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> Result<BeginNavigationResponse, SessionRejection> {
    state.revise_view(
        &payload.pane_id,
        &payload.tab_id,
        payload.session_id,
        payload.view,
    )
}

/// Idempotent release of a v2 directory session. Safe to call more than once
/// (e.g. tab close racing app teardown) — a release for an
/// already-superseded/absent session simply reports `released: false`.
#[tauri::command]
pub fn release_directory_session(
    payload: ReleaseSessionRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> ReleaseSessionResponse {
    state.release_session(&payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn list_tree_children(
    payload: ListTreeChildrenRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ListTreeChildrenResponse, String> {
    let key = payload.path.clone();
    latency_filesystem(&executor, key, move || list_tree_children_impl(payload)).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_tree_children(
    payload: ListTreeChildrenRequest,
) -> Result<ListTreeChildrenResponse, String> {
    list_tree_children_impl(payload)
}

fn list_tree_children_impl(
    payload: ListTreeChildrenRequest,
) -> Result<ListTreeChildrenResponse, String> {
    fs::list_tree_children(&payload).map_err(|error| {
        let message = format!(
            "Failed to load tree children for \"{}\": {error}",
            payload.path
        );
        log::error!("{message}");
        message
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn create_folder(
    payload: CreateEntryRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<DirectoryEntry, String> {
    let key = payload.parent.clone();
    latency_filesystem_cancellable(&executor, key, move |cancel| {
        if cancel.is_cancelled() {
            return Err("create folder cancelled".to_string());
        }
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
        let message = format!("Failed to create folder \"{}\": {error}", payload.name);
        log::error!("{message}");
        message
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn create_file(
    payload: CreateEntryRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<DirectoryEntry, String> {
    let key = payload.parent.clone();
    latency_filesystem_cancellable(&executor, key, move |cancel| {
        if cancel.is_cancelled() {
            return Err("create file cancelled".to_string());
        }
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
        let message = format!("Failed to create file \"{}\": {error}", payload.name);
        log::error!("{message}");
        message
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn rename_entry(
    payload: RenameEntryRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<DirectoryEntry, String> {
    let key = payload.path.clone();
    latency_filesystem_cancellable(&executor, key, move |cancel| {
        if cancel.is_cancelled() {
            return Err("rename cancelled".to_string());
        }
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
    fs::rename_entry(&payload.path, &payload.new_name).map_err(|error| {
        let message = format!("Failed to rename \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

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
    crate::trash::move_to_trash(&payload.paths).map_err(|error| {
        let message = format!("Failed to move items to trash: {error}");
        log::error!("{message}");
        message
    })
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
    crate::trash::restore_from_trash(&payload.ids).map_err(|error| {
        let message = format!("Failed to restore items from trash: {error}");
        log::error!("{message}");
        message
    })
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
    crate::trash::empty_trash().map_err(|error| {
        let message = format!("Failed to empty trash: {error}");
        log::error!("{message}");
        message
    })
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
        let message = format!("Failed to permanently delete items from trash: {error}");
        log::error!("{message}");
        message
    })
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn open_path(
    payload: OpenPathRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<(), String> {
    let key = payload.path.clone();
    latency_filesystem(&executor, key, move || open_path_impl(payload)).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn open_path(payload: OpenPathRequest) -> Result<(), String> {
    open_path_impl(payload)
}

fn open_path_impl(payload: OpenPathRequest) -> Result<(), String> {
    crate::launch::open_path(Path::new(&payload.path)).map_err(|error| {
        let message = format!("Failed to open \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

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
            // LaunchServices may keep its OS-owned confirmation UI open for
            // longer than the normal short IPC deadline. The macOS adapter
            // still has its own 10-second completion safeguard, so this is a
            // margin for scheduling/serialization rather than unbounded work.
            Duration::from_secs(15),
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

/// Reads the registry's current snapshot; performs no platform volume
/// discovery on this path. The registry itself keeps the snapshot current
/// via its long-lived native registrations and the window-focus reconcile
/// safety net (see `lib.rs`), so a plain command invocation here is always a
/// read of already-owned state, matching the IPC execution matrix's "bounded
/// reads of already-owned immutable memory" carve-out.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn list_volumes(
    registry: State<'_, Arc<volumes::registry::VolumeRegistry>>,
) -> Result<Vec<VolumeInfo>, String> {
    Ok(registry.snapshot().to_volume_infos())
}

/// Under `test-utils`, no `VolumeRegistry` is managed as Tauri state (the
/// registry's `test-utils` variant is exercised directly by
/// `volume_registry_integration.rs`, not through IPC state management), so
/// this keeps the previous direct-enumeration behavior — reading the same
/// deterministic fixture inventory `volumes::list_volumes()` always returns
/// under `test-utils`.
#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_volumes() -> Result<Vec<VolumeInfo>, String> {
    Ok(volumes::list_volumes())
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn eject_volume(
    registry: State<'_, Arc<volumes::registry::VolumeRegistry>>,
    payload: EjectVolumeRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<MenuActionStatus, String> {
    let mount_root = payload.mount_root;
    let status = latency_filesystem(&executor, mount_root.clone(), move || {
        volumes::eject::eject_volume(&mount_root)
    })
    .await?;
    if status.handled {
        // A successful eject changes the mounted inventory; ask the
        // registry to re-enumerate and publish (bounded by its own refresh
        // deadline) rather than reading/emitting a one-off snapshot here.
        registry.refresh();
    }
    Ok(status)
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn eject_volume(payload: EjectVolumeRequest) -> Result<MenuActionStatus, String> {
    Ok(volumes::eject::eject_volume(&payload.mount_root))
}

#[tauri::command]
pub fn everything_status(state: State<'_, SizeService>) -> super::types::EverythingStatus {
    state.everything_status()
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
    state.request_paths(payload.paths, move |updates| {
        let batch = updates
            .into_iter()
            .map(size_state_event_from_update)
            .collect::<Vec<_>>();
        if !batch.is_empty() {
            let _ = app_handle.emit(super::events::SIZE_STATE, batch);
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

    state.request_paths(payload.paths, move |batch| {
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
    while Instant::now() < deadline {
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
                )
            })
            .count();
        if terminal_count >= expected_paths {
            return recorded.clone();
        }
        drop(recorded);
        std::thread::sleep(Duration::from_millis(10));
    }

    let recorded = updates.lock().expect("size updates lock").clone();
    recorded
}

fn size_state_event_from_update(update: crate::size::SizeUpdate) -> SizeStateEvent {
    SizeStateEvent {
        path: update.path,
        state: update.state,
        source: update.source,
        size_bytes: update.size_bytes,
    }
}

/// Resolves and emits icons for `paths`, batching the `icon://state` event
/// rather than emitting one event per path. This covers both the sequential
/// fallback (used when the Windows rayon icon pool is unavailable, and on
/// macOS) and the pooled path below; both flush via the shared
/// [`IconBatcher`] (every [`crate::ipc::icon_batch::MAX_BATCH`] icons or
/// [`crate::ipc::icon_batch::FLUSH_INTERVAL`], whichever comes first), plus a
/// final flush of any remainder.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_icons(
    payload: RequestIconsRequest,
    app: AppHandle,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) {
    let Some(pool) = crate::file_icons::icon_pool() else {
        let app_handle = app.clone();
        let executor = Arc::clone(executor.inner());
        // The fallback must not resolve native icons on the IPC dispatcher.
        // It has the same event contract as the Rayon path, but is owned by
        // the bounded latency executor when the specialized icon pool cannot
        // be created.
        tauri::async_runtime::spawn(async move {
            let _ = executor
                .latency("icons".into(), move || {
                    emit_icons(payload.paths, &app_handle);
                    Ok(())
                })
                .await;
        });
        return;
    };

    let app_handle = app.clone();
    pool.spawn(move || {
        let batcher = Mutex::new(IconBatcher::new(Instant::now()));
        payload.paths.into_par_iter().for_each(|path| {
            let icon_data_url = crate::file_icons::resolve_icon(Path::new(&path), false);
            let event = IconStateEvent {
                path,
                icon_data_url,
            };
            let flushed = batcher
                .lock()
                .expect("icon batcher lock")
                .push(event, Instant::now());
            if let Some(batch) = flushed {
                let _ = app_handle.emit(super::events::ICON_STATE, batch);
            }
        });

        let remainder = batcher
            .into_inner()
            .expect("icon batcher lock")
            .drain_remainder();
        if let Some(batch) = remainder {
            let _ = app_handle.emit(super::events::ICON_STATE, batch);
        }
    });
}

#[cfg(not(feature = "test-utils"))]
fn emit_icons(paths: Vec<String>, app: &AppHandle) {
    let mut batcher = IconBatcher::new(Instant::now());
    for path in paths {
        let icon_data_url = crate::file_icons::resolve_icon(Path::new(&path), false);
        let event = IconStateEvent {
            path,
            icon_data_url,
        };
        if let Some(batch) = batcher.push(event, Instant::now()) {
            let _ = app.emit(super::events::ICON_STATE, batch);
        }
    }
    if let Some(batch) = batcher.drain_remainder() {
        let _ = app.emit(super::events::ICON_STATE, batch);
    }
}

/// `test-utils` synchronous variant: returns the same batched shape the
/// production command emits over IPC (`Vec<Vec<IconStateEvent>>`, one inner
/// `Vec` per flushed batch) instead of a single flat list, so tests can
/// assert on the number of batches produced.
#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_icons(payload: RequestIconsRequest) -> Vec<Vec<IconStateEvent>> {
    let mut batcher = IconBatcher::new(Instant::now());
    let mut batches = Vec::new();
    for path in payload.paths {
        let event = IconStateEvent {
            path,
            icon_data_url: None,
        };
        if let Some(batch) = batcher.push(event, Instant::now()) {
            batches.push(batch);
        }
    }
    if let Some(batch) = batcher.drain_remainder() {
        batches.push(batch);
    }
    batches
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn request_visible_item_counts(
    payload: VisibleItemCountsRequest,
    app: AppHandle,
    state: State<'_, ItemCountService>,
    session_state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) {
    let generation = session_state
        .watch_revision_for_pane_path(&payload.context.pane_id, &payload.context.path)
        .unwrap_or(payload.context.request_id);
    let plan = state.plan_automatic_request_with_generation(&payload, generation);
    if plan.is_empty() {
        return;
    }

    if !state.enqueue_automatic_request(plan) {
        return;
    }
    let executor = Arc::clone(executor.inner());
    tauri::async_runtime::spawn(async move {
        // `enqueue_automatic_request` grants at most two drainers; every
        // drainer itself runs in the fixed IPC owner rather than a new OS
        // thread per viewport request.
        let _ = executor
            .latency("item-counts".into(), move || {
                let service = app.state::<ItemCountService>();
                service.process_automatic_queue(|event| {
                    let _ = app.emit(super::events::ITEM_COUNT, event);
                });
                Ok(())
            })
            .await;
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
    state: State<'_, ItemCountService>,
    session_state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ActiveItemsSortResponse, String> {
    let key = payload.context.path.clone();
    let item_counts = state.inner().clone();
    let sessions = Arc::clone(session_state.inner());
    executor
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

/// Folds a legacy [`crate::watch::DirPatch`] into the v2
/// `DirectorySessionService`, if the reporting tab has an active v2 session
/// for the corresponding pane (Phase 5: closes the gap where watch-driven
/// mutations previously left `PaneEntryCollection` stale until the next full
/// reload — see the Phase 4 stopgap notes). Reuses `WatchService`'s
/// already-tested long-lived per-tab watch as the mutation source rather than
/// standing up a second independent long-lived watcher; the v2 session only
/// consumes the *result*, computing its own Rust-authoritative
/// [`SessionPatch`] rather than trusting the legacy patch's already-applied
/// v1 ordering. Returns `None` when there is no active v2 session for the
/// pane, the path does not match, or nothing changed in the derived view —
/// all silent no-ops, matching "stale/path-mismatched patches affect neither
/// pane nor tree" (this function only ever *adds* a session patch on top of
/// the legacy patch, never suppresses it).
pub fn fold_dir_patch_into_session(
    session_state: &crate::directory_session::DirectorySessionService,
    item_count_state: &ItemCountService,
    patch: &crate::watch::DirPatch,
) -> Option<super::types::SessionPatch> {
    fold_dir_patch_into_session_core(session_state, item_count_state, patch)
}

/// Cfg-agnostic core shared by the real (non-`test-utils`) `set_tab_watch`
/// callback and [`fold_dir_patch_into_session_for_tests`], so the fold ->
/// invalidate pipeline is exercised identically by both — see
/// [`fold_dir_patch_into_session`]'s docs for the full contract.
fn fold_dir_patch_into_session_core(
    session_state: &crate::directory_session::DirectorySessionService,
    item_count_state: &ItemCountService,
    patch: &crate::watch::DirPatch,
) -> Option<super::types::SessionPatch> {
    let pane_id = crate::watch::pane_scope(&patch.tab_id).to_string();
    let changed_entries: Vec<DirectoryEntry> = patch
        .changed
        .iter()
        .filter_map(|change| change.entry.clone())
        .collect();
    let changed_dirs: Vec<String> = patch
        .changed
        .iter()
        .filter_map(|change| {
            change
                .entry
                .as_ref()
                .filter(|entry| entry.is_dir)
                .map(|entry| entry.path.clone())
        })
        .collect();
    let session_patch = session_state.apply_watch_mutation(
        &pane_id,
        &patch.path,
        changed_entries,
        patch.removed.clone(),
    )?;
    invalidate_item_counts_for_patch(item_count_state, &session_patch, &changed_dirs);
    Some(session_patch)
}

/// Matches invalidation breadth to a folded [`super::types::SessionPatch`]:
/// an unambiguous `Delta` (single direct-child add/remove/update) only makes
/// the mutated directory's own item count stale (its child count changed by
/// exactly one), so only that directory is invalidated. A `ReplaceView`
/// (multiple or order-ambiguous changes) additionally invalidates every
/// changed *directory* row itself, since a broader/ambiguous batch cannot
/// prove those child directories' own cached counts are still accurate
/// (e.g. a rename-in-place can carry a different identity than the cache
/// key remembers). `MetadataOnly` patches never touch child membership, so
/// they are intentionally left alone (Items counts are unaffected by a
/// field-only update).
///
/// The generation key is each patch's post-mutation `watch_revision` — a
/// per-session, mutation-driven counter that only advances when
/// `apply_watch_mutation`/`resnapshot` actually changes the session's
/// derived view (see `directory_session::mod`'s `ActiveSession::baseline`).
/// Unlike `process_automatic_request`'s `request_id` (a frontend-supplied
/// per-request counter that happens to always be "fresh" because every
/// viewport request gets a new one), `watch_revision` is the only signal in
/// this codebase that actually represents "this directory's on-disk content
/// generation as last observed by the owning session" — exactly the
/// invariant `ItemCountCache::invalidate_generation` needs to drop stale
/// counts without needing every viewport request to also relitigate
/// freshness.
fn invalidate_item_counts_for_patch(
    item_count_state: &ItemCountService,
    patch: &super::types::SessionPatch,
    changed_dirs: &[String],
) {
    use super::types::SessionPatch;

    match patch {
        SessionPatch::Delta {
            path,
            next_baseline,
            ..
        } => {
            item_count_state.invalidate_directory_generation(path, next_baseline.watch_revision.0);
        }
        SessionPatch::ReplaceView {
            path,
            next_baseline,
            ..
        } => {
            let generation = next_baseline.watch_revision.0;
            item_count_state.invalidate_directory_generation(path, generation);
            for changed_dir in changed_dirs {
                item_count_state.invalidate_directory_generation(changed_dir, generation);
            }
        }
        SessionPatch::MetadataOnly { .. } => {}
    }
}

/// Test-only entry point that exercises the exact same watch-patch ->
/// session-fold -> item-count-invalidation pipeline as the real (non-
/// `test-utils`) `set_tab_watch` callback above, without requiring a live
/// `AppHandle`/event emission. `fold_dir_patch_into_session` itself stays
/// `#[cfg(not(feature = "test-utils"))]`-gated because it is only ever
/// reached through the real Tauri command in production; this wrapper lets
/// integration tests prove the invalidation-breadth contract
/// (`invalidate_item_counts_for_patch`) against a real folded
/// [`crate::watch::patch::SessionPatch`] instead of constructing one by hand.
#[cfg(feature = "test-utils")]
pub fn fold_dir_patch_into_session_for_tests(
    session_state: &crate::directory_session::DirectorySessionService,
    item_count_state: &ItemCountService,
    patch: &crate::watch::DirPatch,
) -> Option<super::types::SessionPatch> {
    fold_dir_patch_into_session_core(session_state, item_count_state, patch)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    app: AppHandle,
    state: State<'_, Arc<WatchService>>,
    session_state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> Result<(), String> {
    let patch_app = app.clone();
    let error_app = app;
    let session_state = Arc::clone(session_state.inner());

    state.set_tab_watch(
        payload.target,
        None,
        payload.entries,
        Arc::new(move |patch| {
            let item_count_state = patch_app.state::<ItemCountService>();
            if let Some(session_patch) =
                fold_dir_patch_into_session(&session_state, &item_count_state, &patch)
            {
                let _ = patch_app.emit(super::events::DIR_SESSION_PATCH, session_patch);
            }
        }),
        Arc::new(move |path, message| {
            log::warn!("watch error for {path}: {message}");
            let _ = error_app.emit(
                super::events::WATCH_ERROR,
                super::types::WatchErrorEvent { path, message },
            );
        }),
    )
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    state: State<'_, Arc<WatchService>>,
) -> Result<(), String> {
    state.set_tab_watch(
        payload.target,
        None,
        payload.entries,
        Arc::new(noop_dir_patch),
        Arc::new(noop_watch_error),
    )
}

#[tauri::command]
pub fn start_op(payload: StartOpRequest, state: State<'_, OpsService>) -> String {
    state.start_op(payload)
}

#[tauri::command]
pub fn pause_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.pause_op(&payload.id);
}

#[tauri::command]
pub fn resume_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.resume_op(&payload.id);
}

#[tauri::command]
pub fn cancel_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.cancel_op(&payload.id);
}

#[tauri::command]
pub fn retry_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.retry_op(&payload.id);
}

#[tauri::command]
pub fn reorder_ops(payload: ReorderOpsRequest, state: State<'_, OpsService>) {
    state.reorder_ops(&payload.ids);
}

#[tauri::command]
pub fn resolve_conflict(payload: ResolveConflictRequest, state: State<'_, OpsService>) {
    state.resolve_conflict(
        &payload.id,
        payload.resolution,
        payload.apply_to_all,
        payload.rename_to,
    );
}

#[tauri::command]
pub fn queue_snapshot(state: State<'_, OpsService>) -> Vec<OpSnapshot> {
    state.snapshot()
}

#[tauri::command]
pub fn has_unfinished_ops(state: State<'_, OpsService>) -> bool {
    state.has_unfinished_work()
}

#[tauri::command]
pub fn load_config(state: State<'_, PersistenceState>) -> AppConfig {
    state.config.current()
}

#[tauri::command]
pub fn save_config(payload: SaveConfigRequest, state: State<'_, PersistenceState>) -> AppConfig {
    state.config.replace(payload.config.clone());
    payload.config
}

#[tauri::command]
pub fn load_session(state: State<'_, PersistenceState>) -> SessionState {
    state.session.current()
}

#[tauri::command]
pub fn save_session(
    payload: SaveSessionRequest,
    state: State<'_, PersistenceState>,
) -> SessionState {
    state.session.replace(payload.session.clone());
    payload.session
}

/// Map a frontend level string to a [`log::Level`].
///
/// The frontend permits `trace`, which is folded into `Debug`. Unknown values
/// default to `Info`.
pub fn frontend_log_level(level: &str) -> log::Level {
    match level {
        "error" => log::Level::Error,
        "warn" => log::Level::Warn,
        "debug" | "trace" => log::Level::Debug,
        _ => log::Level::Info,
    }
}

/// Compose a single log line from a frontend log request. `category` defaults to
/// `frontend`; serialized `details` (when present) are appended to the message.
pub fn format_frontend_log(category: Option<&str>, message: &str, details: Option<&str>) -> String {
    let category = category.unwrap_or("frontend");
    match details {
        Some(details) => format!("[{category}] {message} {details}"),
        None => format!("[{category}] {message}"),
    }
}

/// Forward a frontend log line into the application logger (the custom file sink
/// in [`crate::logging`]).
///
/// The frontend logger (`app-log-commands.ts`) routes every line here so test
/// runs stay quiet (the line goes to the IPC sink, not the test console) and so
/// frontend diagnostics share the backend's stdout/webview log targets.
#[tauri::command]
pub fn log_frontend(payload: LogFrontendRequest) {
    let line = format_frontend_log(
        payload.category.as_deref(),
        &payload.message,
        payload.details.as_deref(),
    );
    log::log!(frontend_log_level(&payload.level), "{line}");
}

/// Apply a new minimum capture level: update the live logger and persist the
/// choice into the app config so it survives restarts. Pure core shared by the
/// command wrapper and tests.
pub fn apply_log_level(
    config: &PersistedStore<crate::persist::Config>,
    logger: &crate::logging::FileLogger,
    level: crate::logging::LogLevel,
) {
    logger.set_level(level);
    let mut current = config.current();
    current.log_level = level.as_str().to_string();
    config.replace(current);
}

/// Read and parse the current day's log file for the in-app log viewer.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn read_logs(
    logging: State<'_, Arc<crate::logging::LoggingState>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<Vec<crate::logging::LogEntry>, String> {
    let logging = Arc::clone(logging.inner());
    latency_filesystem(&executor, "logs".into(), move || {
        crate::logging::read_current_day_logs(&logging.dir, crate::logging::current_local_date())
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn read_logs(
    logging: State<'_, crate::logging::LoggingState>,
) -> Vec<crate::logging::LogEntry> {
    crate::logging::read_current_day_logs(&logging.dir, crate::logging::current_local_date())
}

/// Set the minimum backend capture level (applied immediately and persisted).
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn set_log_level(
    payload: SetLogLevelRequest,
    logging: State<'_, Arc<crate::logging::LoggingState>>,
    persistence: State<'_, PersistenceState>,
) -> Result<(), String> {
    let parsed = crate::logging::LogLevel::parse(&payload.level)
        .ok_or_else(|| format!("invalid log level: {}", payload.level))?;
    apply_log_level(&persistence.config, &logging.logger, parsed);
    Ok(())
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn set_log_level(
    payload: SetLogLevelRequest,
    logging: State<'_, crate::logging::LoggingState>,
    persistence: State<'_, PersistenceState>,
) -> Result<(), String> {
    let parsed = crate::logging::LogLevel::parse(&payload.level)
        .ok_or_else(|| format!("invalid log level: {}", payload.level))?;
    apply_log_level(&persistence.config, &logging.logger, parsed);
    Ok(())
}
