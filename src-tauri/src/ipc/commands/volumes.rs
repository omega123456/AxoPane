use crate::size::SizeService;
use crate::volumes;
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem;
use crate::ipc::types::{EjectVolumeRequest, MenuActionStatus, VolumeInfo};
use tauri::State;

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn list_volumes(
    registry: State<'_, Arc<volumes::registry::VolumeRegistry>>,
) -> Result<Vec<VolumeInfo>, String> {
    Ok(registry.snapshot().to_volume_infos())
}

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
