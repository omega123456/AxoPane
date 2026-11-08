use crate::size::SizeService;
use crate::volumes;
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem;
use crate::ipc::types::{EjectVolumeRequest, MenuActionStatus, VolumeInfo};
use tauri::State;

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
pub fn folder_size_index_status(
    state: State<'_, SizeService>,
) -> crate::ipc::types::FolderSizeIndexStatus {
    state.folder_size_index_status()
}
