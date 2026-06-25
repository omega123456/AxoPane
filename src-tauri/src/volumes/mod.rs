use serde::{Deserialize, Serialize};

#[cfg(not(feature = "test-utils"))]
use sysinfo::Disks;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub mount_root: String,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub is_network: bool,
}

#[cfg(not(feature = "test-utils"))]
pub fn list_volumes() -> Vec<VolumeInfo> {
    let disks = Disks::new_with_refreshed_list();

    disks
        .list()
        .iter()
        .map(|disk| VolumeInfo {
            mount_root: disk.mount_point().to_string_lossy().into_owned(),
            label: disk.name().to_string_lossy().into_owned(),
            total_bytes: disk.total_space(),
            free_bytes: disk.available_space(),
            is_network: is_network_disk(disk.mount_point().to_string_lossy().as_ref(), disk.file_system().to_string_lossy().as_ref()),
        })
        .collect()
}

#[cfg(feature = "test-utils")]
pub fn list_volumes() -> Vec<VolumeInfo> {
    test_volumes()
}

#[cfg(feature = "test-utils")]
fn test_volumes() -> Vec<VolumeInfo> {
    if cfg!(windows) {
        return vec![
            VolumeInfo {
                mount_root: String::from("C:\\"),
                label: String::from("Fixture System"),
                total_bytes: 1_000_000,
                free_bytes: 400_000,
                is_network: false,
            },
            VolumeInfo {
                mount_root: String::from("\\\\fixture\\share"),
                label: String::from("Fixture Network"),
                total_bytes: 2_000_000,
                free_bytes: 1_500_000,
                is_network: true,
            },
        ];
    }

    vec![
        VolumeInfo {
            mount_root: String::from("/"),
            label: String::from("Fixture Root"),
            total_bytes: 1_000_000,
            free_bytes: 400_000,
            is_network: false,
        },
        VolumeInfo {
            mount_root: String::from("/Volumes/fixture-network"),
            label: String::from("Fixture Network"),
            total_bytes: 2_000_000,
            free_bytes: 1_500_000,
            is_network: true,
        },
    ]
}

pub fn path_is_network(path: &std::path::Path, volumes: &[VolumeInfo]) -> bool {
    let path_text = path.to_string_lossy().to_ascii_lowercase();

    volumes
        .iter()
        .filter(|volume| {
            let mount = volume.mount_root.to_ascii_lowercase();
            path_text == mount
                || path_text
                    .strip_prefix(&mount)
                    .is_some_and(|remainder| remainder.starts_with('\\') || remainder.starts_with('/'))
        })
        .max_by_key(|volume| volume.mount_root.len())
        .is_some_and(|volume| volume.is_network)
}

#[cfg(not(feature = "test-utils"))]
fn is_network_disk(mount_root: &str, file_system: &str) -> bool {
    #[cfg(windows)]
    if mount_root.starts_with("\\\\") {
        return true;
    }

    matches!(
        file_system.to_ascii_lowercase().as_str(),
        "nfs" | "smbfs" | "cifs" | "afpfs" | "sshfs" | "webdav"
    )
}
