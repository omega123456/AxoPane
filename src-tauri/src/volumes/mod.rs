use serde::{Deserialize, Serialize};

#[cfg(not(feature = "test-utils"))]
use std::cmp::Ordering;

#[cfg(not(feature = "test-utils"))]
use std::sync::{
    atomic::{AtomicBool, Ordering as AtomicOrdering},
    Arc, Mutex,
};

#[cfg(not(feature = "test-utils"))]
use std::thread::{self, JoinHandle};

#[cfg(not(feature = "test-utils"))]
use std::time::Duration;

#[cfg(all(not(feature = "test-utils"), not(windows)))]
use sysinfo::Disks;

#[cfg(all(not(feature = "test-utils"), windows))]
use std::ffi::{c_void, OsStr};

#[cfg(all(not(feature = "test-utils"), windows))]
use std::path::Path;

#[cfg(all(not(feature = "test-utils"), windows))]
use std::os::windows::ffi::OsStrExt;

#[cfg(all(not(feature = "test-utils"), windows))]
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, HANDLE};

#[cfg(all(not(feature = "test-utils"), windows))]
use windows_sys::Win32::NetworkManagement::WNet::{
    WNetCloseEnum, WNetEnumResourceW, WNetOpenEnumW, NETRESOURCEW, RESOURCETYPE_DISK,
    RESOURCE_CONNECTED, RESOURCE_REMEMBERED,
};

#[cfg(all(not(feature = "test-utils"), windows))]
use windows::core::{Interface, PCWSTR};

#[cfg(all(not(feature = "test-utils"), windows))]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};

#[cfg(all(not(feature = "test-utils"), windows))]
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_UNCPRIORITY};

#[cfg(all(not(feature = "test-utils"), windows))]
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter};

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
    pub is_removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct VolumeIdentity {
    mount_root: String,
    label: String,
    total_bytes: u64,
    is_network: bool,
    is_removable: bool,
}

#[cfg(not(feature = "test-utils"))]
const VOLUME_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(not(feature = "test-utils"))]
pub struct VolumeMonitorService {
    started: AtomicBool,
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

#[cfg(not(feature = "test-utils"))]
impl Default for VolumeMonitorService {
    fn default() -> Self {
        Self {
            started: AtomicBool::new(false),
            stop: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
        }
    }
}

#[cfg(not(feature = "test-utils"))]
impl VolumeMonitorService {
    pub fn start(&self, app: AppHandle) {
        if self.started.swap(true, AtomicOrdering::SeqCst) {
            return;
        }

        self.stop.store(false, AtomicOrdering::SeqCst);
        let stop = Arc::clone(&self.stop);

        match thread::Builder::new()
            .name("volume-monitor".to_string())
            .spawn(move || {
                let mut previous = list_volumes();

                while !stop.load(AtomicOrdering::SeqCst) {
                    thread::sleep(VOLUME_POLL_INTERVAL);
                    if stop.load(AtomicOrdering::SeqCst) {
                        break;
                    }

                    let next = list_volumes();
                    if !volume_inventory_changed(&previous, &next) {
                        continue;
                    }

                    previous = next.clone();
                    if let Err(error) = app.emit(
                        crate::ipc::events::VOLUMES_CHANGED,
                        crate::ipc::types::VolumesChangedEvent { volumes: next },
                    ) {
                        log::error!("volume monitor failed to emit update: {error}");
                    }
                }
            }) {
            Ok(handle) => {
                *self.handle.lock().expect("volume monitor handle lock") = Some(handle);
            }
            Err(error) => {
                self.started.store(false, AtomicOrdering::SeqCst);
                log::error!("failed to start volume monitor: {error}");
            }
        }
    }
}

#[cfg(not(feature = "test-utils"))]
impl Drop for VolumeMonitorService {
    fn drop(&mut self) {
        self.stop.store(true, AtomicOrdering::SeqCst);
        if let Some(handle) = self
            .handle
            .get_mut()
            .expect("volume monitor handle lock")
            .take()
        {
            let _ = handle.join();
        }
    }
}

fn volume_identity(volume: &VolumeInfo) -> VolumeIdentity {
    VolumeIdentity {
        mount_root: volume.mount_root.to_ascii_lowercase(),
        label: volume.label.clone(),
        total_bytes: volume.total_bytes,
        is_network: volume.is_network,
        is_removable: volume.is_removable,
    }
}

pub fn volume_inventory_changed(previous: &[VolumeInfo], next: &[VolumeInfo]) -> bool {
    let mut previous_identities: Vec<_> = previous.iter().map(volume_identity).collect();
    let mut next_identities: Vec<_> = next.iter().map(volume_identity).collect();
    previous_identities.sort();
    next_identities.sort();
    previous_identities != next_identities
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
            is_removable: matches!(drive_type, DRIVE_REMOVABLE | DRIVE_CDROM),
        });
    }

    extend_network_resources(&mut volumes);
    extend_network_shortcuts(&mut volumes);
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
            is_removable: disk.is_removable(),
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
                is_removable: false,
            },
            VolumeInfo {
                mount_root: String::from("D:\\"),
                label: String::from("Fixture Data"),
                total_bytes: 2_000_000,
                free_bytes: 1_100_000,
                is_network: false,
                is_removable: true,
            },
            VolumeInfo {
                mount_root: String::from("Z:\\"),
                label: String::from("Fixture Network"),
                total_bytes: 3_000_000,
                free_bytes: 2_200_000,
                is_network: true,
                is_removable: false,
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
            is_removable: false,
        },
        VolumeInfo {
            mount_root: String::from("/Volumes/fixture-network"),
            label: String::from("Fixture Network"),
            total_bytes: 2_000_000,
            free_bytes: 1_500_000,
            is_network: true,
            is_removable: false,
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

#[cfg(all(not(feature = "test-utils"), windows))]
fn extend_network_resources(volumes: &mut Vec<VolumeInfo>) {
    extend_network_resources_for_scope(volumes, RESOURCE_CONNECTED);
    extend_network_resources_for_scope(volumes, RESOURCE_REMEMBERED);
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn extend_network_resources_for_scope(volumes: &mut Vec<VolumeInfo>, scope: u32) {
    let mut handle: HANDLE = std::ptr::null_mut();
    let status =
        unsafe { WNetOpenEnumW(scope, RESOURCETYPE_DISK, 0, std::ptr::null(), &mut handle) };

    if status != ERROR_SUCCESS {
        return;
    }

    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        let mut count = u32::MAX;
        let mut buffer_size = buffer.len() as u32;
        let status = unsafe {
            WNetEnumResourceW(
                handle,
                &mut count,
                buffer.as_mut_ptr().cast::<c_void>(),
                &mut buffer_size,
            )
        };

        if status == ERROR_NO_MORE_ITEMS {
            break;
        }

        if status == ERROR_MORE_DATA && buffer_size as usize > buffer.len() {
            buffer.resize(buffer_size as usize, 0);
            continue;
        }

        if status != ERROR_SUCCESS {
            break;
        }

        let resources = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr().cast::<NETRESOURCEW>(), count as usize)
        };

        for resource in resources {
            if resource.dwType != RESOURCETYPE_DISK {
                continue;
            }

            let remote_name = wide_ptr_to_string(resource.lpRemoteName);
            let local_name = wide_ptr_to_string(resource.lpLocalName);
            let mount_root = normalize_network_mount_root(&local_name, &remote_name);

            let Some(mount_root) = mount_root else {
                continue;
            };

            if volumes
                .iter()
                .any(|volume| volume.mount_root.eq_ignore_ascii_case(&mount_root))
            {
                continue;
            }

            push_network_volume(volumes, mount_root, None, true);
        }
    }

    unsafe {
        WNetCloseEnum(handle);
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn wide_ptr_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }

    let mut len = 0_usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }

        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
            .trim()
            .to_string()
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn extend_network_shortcuts(volumes: &mut Vec<VolumeInfo>) {
    let Some(root) = std::env::var_os("APPDATA") else {
        return;
    };

    let shortcuts_dir = Path::new(&root)
        .join("Microsoft")
        .join("Windows")
        .join("Network Shortcuts");

    let initialized_com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };

    {
        let Ok(entries) = std::fs::read_dir(shortcuts_dir) else {
            if initialized_com {
                unsafe {
                    CoUninitialize();
                }
            }
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                extend_network_shortcut_dir(volumes, &path);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
            {
                extend_network_shortcut_file(volumes, &path, None);
            }
        }
    }

    if initialized_com {
        unsafe {
            CoUninitialize();
        }
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn extend_network_shortcut_dir(volumes: &mut Vec<VolumeInfo>, path: &Path) {
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string);
    let target_lnk = path.join("target.lnk");

    if target_lnk.exists() {
        extend_network_shortcut_file(volumes, &target_lnk, label.as_deref());
        return;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let shortcut = entry.path();
        if shortcut
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            extend_network_shortcut_file(volumes, &shortcut, label.as_deref());
        }
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn extend_network_shortcut_file(volumes: &mut Vec<VolumeInfo>, path: &Path, label: Option<&str>) {
    if let Some(candidate) = resolve_shell_link_target(path) {
        if let Some(mount_root) = normalize_network_mount_root("", &candidate) {
            push_network_volume(volumes, mount_root, label, false);
        }
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn push_network_volume(
    volumes: &mut Vec<VolumeInfo>,
    mount_root: String,
    label: Option<&str>,
    query_space: bool,
) {
    if volumes
        .iter()
        .any(|volume| volume.mount_root.eq_ignore_ascii_case(&mount_root))
    {
        return;
    }

    let fallback_label = label
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| label_for_unc_root(&mount_root));

    let (total_bytes, free_bytes, label) = if query_space {
        let space_root = to_wide(&space_query_root(&mount_root));
        let (total_bytes, free_bytes) = disk_space_for_root(&space_root);
        let volume_label = read_volume_label(&space_root);
        let label = if volume_label.is_empty() {
            fallback_label
        } else {
            volume_label
        };
        (total_bytes, free_bytes, label)
    } else {
        (0, 0, fallback_label)
    };

    volumes.push(VolumeInfo {
        mount_root,
        label,
        total_bytes,
        free_bytes,
        is_network: true,
        is_removable: false,
    });
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn resolve_shell_link_target(path: &Path) -> Option<String> {
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()? };
    let persist_file: IPersistFile = link.cast().ok()?;
    let link_path = to_wide_path(path);

    unsafe {
        persist_file
            .Load(PCWSTR(link_path.as_ptr()), STGM_READ)
            .ok()?;
    }

    let mut target = [0_u16; 32_768];
    unsafe {
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_UNCPRIORITY.0 as u32)
            .ok()?;
    }

    let end = target
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(target.len());
    let value = String::from_utf16_lossy(&target[..end]).trim().to_string();

    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn to_wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn normalize_network_mount_root(local_name: &str, remote_name: &str) -> Option<String> {
    if let Some(local_root) = normalize_drive_root(local_name) {
        return Some(local_root);
    }

    let trimmed = remote_name.trim().trim_end_matches(['\\', '/']);
    if !trimmed.starts_with("\\\\") {
        return None;
    }

    let parts: Vec<_> = trimmed
        .trim_start_matches('\\')
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect();

    if parts.len() < 2 {
        return None;
    }

    Some(format!("\\\\{}\\{}", parts[0], parts[1]))
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn normalize_drive_root(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches(['\\', '/']);
    let mut chars = trimmed.chars();
    let drive = chars.next()?;
    let colon = chars.next()?;

    if chars.next().is_none() && drive.is_ascii_alphabetic() && colon == ':' {
        Some(format!("{}:\\", drive.to_ascii_uppercase()))
    } else {
        None
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn space_query_root(mount_root: &str) -> String {
    if mount_root.starts_with("\\\\") && !mount_root.ends_with('\\') {
        format!("{mount_root}\\")
    } else {
        mount_root.to_string()
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn label_for_unc_root(mount_root: &str) -> String {
    let parts: Vec<_> = mount_root
        .trim_start_matches('\\')
        .split('\\')
        .filter(|part| !part.is_empty())
        .collect();

    match parts.as_slice() {
        [server, share, ..] => format!("{share} ({server})"),
        _ => mount_root.to_string(),
    }
}

pub fn path_is_network(path: &std::path::Path, volumes: &[VolumeInfo]) -> bool {
    let path_text = path.to_string_lossy().to_ascii_lowercase();

    volumes
        .iter()
        .filter(|volume| {
            let mount = volume.mount_root.to_ascii_lowercase();
            path_text == mount
                || path_text.strip_prefix(&mount).is_some_and(|remainder| {
                    remainder.starts_with('\\') || remainder.starts_with('/')
                })
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
