use serde::{Deserialize, Serialize};

#[cfg(not(feature = "test-utils"))]
use std::cmp::Ordering;

#[cfg(all(not(feature = "test-utils"), not(windows)))]
use sysinfo::Disks;

#[cfg(all(not(feature = "test-utils"), windows))]
use std::ffi::OsStr;

#[cfg(all(not(feature = "test-utils"), windows))]
use std::os::windows::ffi::OsStrExt;

#[cfg(all(not(feature = "test-utils"), windows))]
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

#[cfg(all(not(feature = "test-utils"), windows))]
const DRIVE_UNKNOWN: u32 = 0;
#[cfg(all(not(feature = "test-utils"), windows))]
const DRIVE_NO_ROOT_DIR: u32 = 1;
#[cfg(all(not(feature = "test-utils"), windows))]
const DRIVE_REMOVABLE: u32 = 2;
#[cfg(all(not(feature = "test-utils"), windows))]
const DRIVE_FIXED: u32 = 3;
#[cfg(all(not(feature = "test-utils"), windows))]
const DRIVE_REMOTE: u32 = 4;
#[cfg(all(not(feature = "test-utils"), windows))]
const DRIVE_CDROM: u32 = 5;
#[cfg(all(not(feature = "test-utils"), windows))]
const DRIVE_RAMDISK: u32 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub mount_root: String,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub is_network: bool,
}

#[cfg(all(not(feature = "test-utils"), windows))]
pub fn list_volumes() -> Vec<VolumeInfo> {
    let bitmask = unsafe { GetLogicalDrives() };
    let mut volumes = Vec::new();

    for offset in 0..26_u32 {
        if bitmask & (1 << offset) == 0 {
            continue;
        }

        let drive_letter = char::from_u32(u32::from(b'A') + offset).expect("drive letter");
        let mount_root = format!("{drive_letter}:\\");
        let wide_mount_root = to_wide(&mount_root);
        let drive_type = unsafe { GetDriveTypeW(wide_mount_root.as_ptr()) };

        if matches!(drive_type, DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR) {
            continue;
        }

        if !matches!(
            drive_type,
            DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_REMOTE | DRIVE_CDROM | DRIVE_RAMDISK
        ) {
            continue;
        }

        let (total_bytes, free_bytes) = disk_space_for_root(&wide_mount_root);
        let label = read_volume_label(&wide_mount_root);
        volumes.push(VolumeInfo {
            mount_root,
            label,
            total_bytes,
            free_bytes,
            is_network: drive_type == DRIVE_REMOTE,
        });
    }

    sort_volumes(&mut volumes);
    volumes
}

#[cfg(not(feature = "test-utils"))]
#[cfg(not(windows))]
pub fn list_volumes() -> Vec<VolumeInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut volumes: Vec<_> = disks
        .list()
        .iter()
        .map(|disk| VolumeInfo {
            mount_root: disk.mount_point().to_string_lossy().into_owned(),
            label: disk.name().to_string_lossy().into_owned(),
            total_bytes: disk.total_space(),
            free_bytes: disk.available_space(),
            is_network: is_network_disk(
                disk.mount_point().to_string_lossy().as_ref(),
                disk.file_system().to_string_lossy().as_ref(),
            ),
        })
        .collect();

    sort_volumes(&mut volumes);
    volumes
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
                label: String::from("Fixture Windows"),
                total_bytes: 1_000_000,
                free_bytes: 400_000,
                is_network: false,
            },
            VolumeInfo {
                mount_root: String::from("D:\\"),
                label: String::from("Fixture Data"),
                total_bytes: 2_000_000,
                free_bytes: 1_100_000,
                is_network: false,
            },
            VolumeInfo {
                mount_root: String::from("Z:\\"),
                label: String::from("Fixture Network"),
                total_bytes: 3_000_000,
                free_bytes: 2_200_000,
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

#[cfg(not(feature = "test-utils"))]
fn sort_volumes(volumes: &mut [VolumeInfo]) {
    volumes.sort_by(|left, right| {
        left.mount_root
            .to_ascii_lowercase()
            .cmp(&right.mount_root.to_ascii_lowercase())
            .then_with(|| {
                if left.is_network == right.is_network {
                    Ordering::Equal
                } else if left.is_network {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            })
    });
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn read_volume_label(root: &[u16]) -> String {
    let mut buffer = [0_u16; 261];
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };

    if ok == 0 {
        return String::new();
    }

    let end = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end]).trim().to_string()
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn disk_space_for_root(root: &[u16]) -> (u64, u64) {
    let mut available_to_caller = 0_u64;
    let mut total_bytes = 0_u64;
    let mut free_bytes = 0_u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            root.as_ptr(),
            &mut available_to_caller,
            &mut total_bytes,
            &mut free_bytes,
        )
    };

    if ok == 0 {
        return (0, 0);
    }

    (total_bytes, free_bytes)
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

#[cfg(all(not(feature = "test-utils"), not(windows)))]
fn is_network_disk(_mount_root: &str, file_system: &str) -> bool {
    matches!(
        file_system.to_ascii_lowercase().as_str(),
        "nfs" | "smbfs" | "cifs" | "afpfs" | "sshfs" | "webdav"
    )
}
