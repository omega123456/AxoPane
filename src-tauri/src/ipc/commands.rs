use crate::app_picker;
use crate::archive;
use crate::fs;
use crate::item_counts::ItemCountService;
use crate::ipc::icon_batch::IconBatcher;
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
    AppConfig, CancelSizeRequest, CancelSizeResponse, CancelSizesRequest, CancelSizesResponse,
    CompressArchiveRequest, CreateEntryRequest, DeleteFromTrashRequest, EjectVolumeRequest,
    ExtractArchiveRequest, FileClipboardMode, FolderSizeRequest, FolderSizesRequest,
    GetDefaultApplicationRequest, GetDefaultApplicationResponse, IconStateEvent,
    InitialShellResponse, InvokeNativeMenuRequest, ListApplicationsResponse, ListDirRequest,
    ListDirResponse, ListTrashResponse, ListTreeChildrenRequest, ListTreeChildrenResponse,
    LoadNativeMenuRequest, LoadNativeMenuResponse, LogFrontendRequest, MenuActionStatus,
    OpIdRequest, OpenPathRequest, OpenWithRequest, RenameEntryRequest, ReorderOpsRequest,
    RequestIconsRequest, ResolveConflictRequest, RestoreTrashRequest, SaveConfigRequest,
    ActiveItemsSortRequest, ActiveItemsSortResponse, SaveSessionRequest, SessionState,
    SetDefaultApplicationRequest, SetLogLevelRequest, SetTabWatchRequest, ShowPropertiesRequest,
    SizeStateEvent, StartListDirHead, StartListDirRequest, StartListDirResponse,
    StartListDirSuperseded, StartOpRequest, TrashEntriesRequest, VisibleItemCountsRequest,
    VolumeInfo, WarmNativeMenusRequest, WriteFileClipboardRequest,
};
use crate::fs::{DirectoryEntry, ListDirOutcome};
use crate::listing::ListingService;
use std::path::Path;
use tauri::State;
#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter, Manager};

#[cfg(feature = "test-utils")]
#[inline(never)]
pub fn noop_dir_patch(_: crate::watch::DirPatch) {}

#[cfg(feature = "test-utils")]
#[inline(never)]
pub fn noop_watch_error(_: String, _: String) {}

#[tauri::command]
pub fn get_initial_shell() -> InitialShellResponse {
    mock::initial_shell()
}

#[tauri::command]
pub fn list_dir(payload: ListDirRequest) -> Result<ListDirResponse, String> {
    fs::list_dir(&payload).map_err(|error| {
        let message = format!("Failed to load \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

/// Streams a directory listing to the frontend in chunks.
///
/// Enumerating + sorting the whole directory happens here on a Tauri worker
/// thread (never the UI thread), then the response carries only the inline
/// first chunk; any remaining entries are emitted as `list_chunk` events from a
/// background thread. This keeps the webview from parsing a single multi-MB
/// listing array in one blocking `JSON.parse`. A per-tab request id (via
/// [`ListingService`]) lets a superseding navigation cancel the in-flight
/// stream so abandoned listings stop emitting.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn start_list_dir(
    payload: StartListDirRequest,
    app: AppHandle,
    state: State<'_, ListingService>,
    item_counts: State<'_, ItemCountService>,
) -> Result<StartListDirResponse, String> {
    item_counts.cancel_tab(&payload.tab_id);
    let session = state.begin_session(&payload.tab_id);
    let outcome =
        fs::list_dir_with_cancellation(&payload.to_options(), || state.is_cancelled(&session))
            .map_err(|error| {
                let message = format!("Failed to load \"{}\": {error}", payload.path);
                log::error!("{message}");
                message
            })?;

    let ListDirResponse { path, entries } = match outcome {
        ListDirOutcome::Complete(response) => response,
        ListDirOutcome::Cancelled => {
            return Ok(StartListDirResponse::Superseded(StartListDirSuperseded {
                request_id: session.request_id,
            }));
        }
    };

    let total = entries.len() as u64;
    let request_id = session.request_id;
    if !state.complete_session(
        &session,
        path.clone(),
        payload.sort_key,
        payload.sort_direction,
        payload.filter.clone(),
        payload.show_hidden,
        payload.include_item_counts,
        entries.clone(),
    ) {
        return Ok(StartListDirResponse::Superseded(StartListDirSuperseded {
            request_id,
        }));
    }
    let (first_chunk, rest) =
        crate::listing::split_first_chunk(entries, crate::listing::LIST_CHUNK_SIZE);
    let done = rest.is_empty();

    if !done {
        let tab_id = payload.tab_id.clone();
        let stream_path = path.clone();
        std::thread::spawn(move || {
            let service = app.state::<ListingService>();
            let total_rest = rest.len();
            let mut emitted = 0usize;
            for (chunk_offset, chunk) in rest.chunks(crate::listing::LIST_CHUNK_SIZE).enumerate() {
                // Bail as soon as a newer navigation for this tab supersedes us.
                if !service.is_current(&tab_id, request_id) {
                    return;
                }
                emitted += chunk.len();
                let event = super::types::ListChunkEvent {
                    tab_id: tab_id.clone(),
                    request_id,
                    path: stream_path.clone(),
                    entries: chunk.to_vec(),
                    chunk_index: chunk_offset as u64 + 1,
                    done: emitted >= total_rest,
                };
                let _ = app.emit(super::events::LIST_CHUNK, event);
            }
        });
    }

    Ok(StartListDirResponse::Head(StartListDirHead {
        path,
        total,
        request_id,
        first_chunk,
        done,
    }))
}

/// `test-utils` variant: builds the same listing head (first chunk + totals)
/// without an `AppHandle`, so the streamed remainder is never emitted. The
/// chunk-splitting and request-id logic it shares with the production command
/// live in [`crate::listing`] and are covered directly by integration tests.
#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn start_list_dir(
    payload: StartListDirRequest,
    state: State<'_, ListingService>,
    item_counts: State<'_, ItemCountService>,
) -> Result<StartListDirResponse, String> {
    item_counts.cancel_tab(&payload.tab_id);
    let session = state.begin_session(&payload.tab_id);
    let outcome =
        fs::list_dir_with_cancellation(&payload.to_options(), || state.is_cancelled(&session))
            .map_err(|error| error.to_string())?;

    let ListDirResponse { path, entries } = match outcome {
        ListDirOutcome::Complete(response) => response,
        ListDirOutcome::Cancelled => {
            return Ok(StartListDirResponse::Superseded(StartListDirSuperseded {
                request_id: session.request_id,
            }));
        }
    };

    let total = entries.len() as u64;
    let request_id = session.request_id;
    if !state.complete_session(
        &session,
        path.clone(),
        payload.sort_key,
        payload.sort_direction,
        payload.filter.clone(),
        payload.show_hidden,
        payload.include_item_counts,
        entries.clone(),
    ) {
        return Ok(StartListDirResponse::Superseded(StartListDirSuperseded {
            request_id,
        }));
    }
    let (first_chunk, rest) =
        crate::listing::split_first_chunk(entries, crate::listing::LIST_CHUNK_SIZE);

    Ok(StartListDirResponse::Head(StartListDirHead {
        path,
        total,
        request_id,
        done: rest.is_empty(),
        first_chunk,
    }))
}

#[tauri::command]
pub fn list_tree_children(
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

#[tauri::command]
pub fn create_folder(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    fs::create_directory(&payload.parent, &payload.name).map_err(|error| {
        let message = format!("Failed to create folder \"{}\": {error}", payload.name);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn create_file(payload: CreateEntryRequest) -> Result<DirectoryEntry, String> {
    fs::create_file(&payload.parent, &payload.name).map_err(|error| {
        let message = format!("Failed to create file \"{}\": {error}", payload.name);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn rename_entry(payload: RenameEntryRequest) -> Result<DirectoryEntry, String> {
    fs::rename_entry(&payload.path, &payload.new_name).map_err(|error| {
        let message = format!("Failed to rename \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn move_to_trash(payload: TrashEntriesRequest) -> Result<(), String> {
    crate::trash::move_to_trash(&payload.paths).map_err(|error| {
        let message = format!("Failed to move items to trash: {error}");
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn list_trash() -> Result<ListTrashResponse, String> {
    crate::trash::list_trash()
        .map(|entries| ListTrashResponse { entries })
        .map_err(|error| {
            let message = format!("Failed to list trash: {error}");
            log::error!("{message}");
            message
        })
}

#[tauri::command]
pub fn restore_from_trash(payload: RestoreTrashRequest) -> Result<(), String> {
    crate::trash::restore_from_trash(&payload.ids).map_err(|error| {
        let message = format!("Failed to restore items from trash: {error}");
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn empty_trash() -> Result<(), String> {
    crate::trash::empty_trash().map_err(|error| {
        let message = format!("Failed to empty trash: {error}");
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn delete_from_trash(payload: DeleteFromTrashRequest) -> Result<(), String> {
    crate::trash::delete_from_trash(&payload.ids).map_err(|error| {
        let message = format!("Failed to permanently delete items from trash: {error}");
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn open_path(payload: OpenPathRequest) -> Result<(), String> {
    crate::launch::open_path(Path::new(&payload.path)).map_err(|error| {
        let message = format!("Failed to open \"{}\": {error}", payload.path);
        log::error!("{message}");
        message
    })
}

#[tauri::command]
pub fn write_file_clipboard(payload: WriteFileClipboardRequest) -> Result<(), String> {
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

#[tauri::command]
pub fn clear_file_clipboard() -> Result<(), String> {
    crate::clipboard::clear().map_err(|error| {
        let message = format!("Failed to clear OS clipboard: {error}");
        log::warn!("{message}");
        message
    })
}

#[tauri::command]
pub fn load_native_menu(
    payload: LoadNativeMenuRequest,
    state: State<'_, NativeMenuService>,
) -> LoadNativeMenuResponse {
    state.load_menu(payload)
}

#[tauri::command]
pub fn invoke_native_menu_action(
    payload: InvokeNativeMenuRequest,
    state: State<'_, NativeMenuService>,
) -> MenuActionStatus {
    state.invoke_menu_action(payload)
}

#[tauri::command]
pub fn show_properties(
    payload: ShowPropertiesRequest,
    state: State<'_, NativeMenuService>,
) -> MenuActionStatus {
    state.show_properties(payload)
}

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
pub fn warm_native_menus(payload: WarmNativeMenusRequest, state: State<'_, NativeMenuService>) {
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

#[tauri::command]
pub async fn list_applications() -> ListApplicationsResponse {
    app_picker::list_applications().await
}

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

#[tauri::command]
pub fn compress_archive(payload: CompressArchiveRequest) -> MenuActionStatus {
    archive::compress_archive(payload)
}

#[tauri::command]
pub fn extract_archive(payload: ExtractArchiveRequest) -> MenuActionStatus {
    archive::extract_archive(payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn list_volumes(app: AppHandle) -> Result<Vec<VolumeInfo>, String> {
    let volumes = volumes::list_volumes();

    app.emit(
        super::events::VOLUMES_CHANGED,
        super::types::VolumesChangedEvent {
            volumes: volumes.clone(),
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(volumes)
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_volumes() -> Result<Vec<VolumeInfo>, String> {
    Ok(volumes::list_volumes())
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn eject_volume(
    app: AppHandle,
    payload: EjectVolumeRequest,
) -> Result<MenuActionStatus, String> {
    let status = volumes::eject::eject_volume(&payload.mount_root);
    if status.handled {
        let volumes = volumes::list_volumes();
        let _ = app.emit(
            super::events::VOLUMES_CHANGED,
            super::types::VolumesChangedEvent { volumes },
        );
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
pub fn request_icons(payload: RequestIconsRequest, app: AppHandle) {
    let Some(pool) = crate::file_icons::icon_pool() else {
        let app_handle = app.clone();
        let mut batcher = IconBatcher::new(Instant::now());
        for path in payload.paths {
            let icon_data_url = crate::file_icons::resolve_icon(Path::new(&path), false);
            let event = IconStateEvent {
                path,
                icon_data_url,
            };
            if let Some(batch) = batcher.push(event, Instant::now()) {
                let _ = app_handle.emit(super::events::ICON_STATE, batch);
            }
        }
        if let Some(batch) = batcher.drain_remainder() {
            let _ = app_handle.emit(super::events::ICON_STATE, batch);
        }
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
) {
    let plan = state.plan_automatic_request(&payload);
    if plan.is_empty() {
        return;
    }

    if !state.enqueue_automatic_request(plan) {
        return;
    }
    std::thread::spawn(move || {
        let service = app.state::<ItemCountService>();
        service.process_automatic_queue(|event| {
            let _ = app.emit(super::events::ITEM_COUNT, event);
        });
    });
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn request_visible_item_counts(
    payload: VisibleItemCountsRequest,
    state: State<'_, ItemCountService>,
) {
    let plan = state.plan_automatic_request(&payload);
    state.process_automatic_request(plan, |event| state.record_test_event(event));
}

#[tauri::command]
pub fn sort_active_items(
    payload: ActiveItemsSortRequest,
    state: State<'_, ItemCountService>,
) -> Result<ActiveItemsSortResponse, String> {
    state.sort_active_items(&payload).map_err(|error| {
        let message = format!("Failed to sort \"{}\" by Items: {error}", payload.context.path);
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

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    app: AppHandle,
    listing_state: State<'_, ListingService>,
    state: State<'_, WatchService>,
) -> Result<(), String> {
    let patch_app = app.clone();
    let error_app = app;
    let listing_seed_entries = valid_seed_entries(&payload, &listing_state);

    state.set_tab_watch(
        payload.target,
        listing_seed_entries,
        payload.entries,
        Arc::new(move |patch| {
            let _ = patch_app.emit(
                super::events::DIR_PATCH,
                super::types::DirPatchEvent {
                    tab_id: patch.tab_id,
                    path: patch.path,
                    reason: patch.reason,
                    changed: patch
                        .changed
                        .into_iter()
                        .map(|change| super::types::DirEntryPatch {
                            path: change.path,
                            entry: change.entry,
                        })
                        .collect(),
                    removed: patch.removed,
                },
            );
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

fn valid_seed_entries(
    payload: &SetTabWatchRequest,
    listing_state: &ListingService,
) -> Option<Vec<DirectoryEntry>> {
    let target = payload.target.as_ref()?;
    let seed = payload.seed_reference.as_ref()?;
    // A seed is only reusable for the exact watched tab/path context. A stale
    // or cross-tab reference intentionally falls through to WatchService's
    // resnapshot path instead of borrowing an unrelated listing baseline.
    if seed.tab_id != target.tab_id
        || !crate::listing::paths_match_for_context(&seed.path, &target.path)
    {
        return None;
    }
    listing_state.completed_seed_entries(&seed.tab_id, seed.request_id, target)
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn set_tab_watch(
    payload: SetTabWatchRequest,
    listing_state: State<'_, ListingService>,
    state: State<'_, WatchService>,
) -> Result<(), String> {
    let listing_seed_entries = valid_seed_entries(&payload, &listing_state);
    state.set_tab_watch(
        payload.target,
        listing_seed_entries,
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
#[tauri::command]
pub fn read_logs(
    logging: State<'_, crate::logging::LoggingState>,
) -> Vec<crate::logging::LogEntry> {
    crate::logging::read_current_day_logs(&logging.dir, crate::logging::current_local_date())
}

/// Set the minimum backend capture level (applied immediately and persisted).
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
