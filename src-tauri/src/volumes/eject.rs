//! Full safe-remove ("eject") for removable volumes.
//!
//! This is a machine-global operation (it can physically power down a USB
//! device) so it must never touch a real OS API under `test-utils`.

use crate::ipc::types::MenuActionStatus;

#[cfg(all(target_os = "macos", not(feature = "test-utils")))]
pub fn eject_volume(mount_root: &str) -> MenuActionStatus {
    macos::eject_volume(mount_root)
}

#[cfg(all(windows, not(feature = "test-utils")))]
pub fn eject_volume(mount_root: &str) -> MenuActionStatus {
    windows_eject::eject_volume(mount_root)
}

#[cfg(any(not(any(target_os = "macos", windows)), feature = "test-utils"))]
pub fn eject_volume(_mount_root: &str) -> MenuActionStatus {
    MenuActionStatus::unsupported("unsupported")
}

#[cfg(all(target_os = "macos", not(feature = "test-utils")))]
mod macos {
    use crate::ipc::types::MenuActionStatus;
    use crate::native_menu::shell_executor::ShellExecutor;
    use std::process::Command;

    pub fn eject_volume(mount_root: &str) -> MenuActionStatus {
        let mount_root = mount_root.to_string();
        let executor = ShellExecutor::default();
        executor.execute(move || run_diskutil_eject(&mount_root))
    }

    fn run_diskutil_eject(mount_root: &str) -> MenuActionStatus {
        match Command::new("diskutil")
            .arg("eject")
            .arg(mount_root)
            .output()
        {
            Ok(output) if output.status.success() => {
                MenuActionStatus::handled_with_message("ejected")
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let message = if stderr.is_empty() {
                    "eject-failed".to_string()
                } else {
                    stderr
                };
                log::warn!("diskutil eject failed for {mount_root}: {message}");
                MenuActionStatus {
                    handled: false,
                    message: Some(message),
                }
            }
            Err(error) => {
                log::warn!("diskutil eject spawn failed for {mount_root}: {error}");
                MenuActionStatus::unsupported("eject-launch-failed")
            }
        }
    }
}

#[cfg(all(windows, not(feature = "test-utils")))]
mod windows_eject {
    use crate::ipc::types::MenuActionStatus;
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Parent, CM_Request_Device_EjectW, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
        DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, GUID_DEVINTERFACE_DISK, HDEVINFO, PNP_VETO_TYPE,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    };
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER};
    use windows::Win32::System::IO::DeviceIoControl;

    pub fn eject_volume(mount_root: &str) -> MenuActionStatus {
        let Some(drive_letter) = drive_letter(mount_root) else {
            return MenuActionStatus::unsupported("unsupported");
        };

        let Some(device_number) = device_number_for_path(&format!("\\\\.\\{drive_letter}:")) else {
            return MenuActionStatus::unsupported("device-not-found");
        };

        let Some(parent_devinst) = find_parent_devinst(device_number) else {
            return MenuActionStatus::unsupported("device-not-found");
        };

        request_eject(parent_devinst)
    }

    fn drive_letter(mount_root: &str) -> Option<char> {
        let trimmed = mount_root.trim();
        let mut chars = trimmed.chars();
        let letter = chars.next()?;
        if !letter.is_ascii_alphabetic() || chars.next()? != ':' {
            return None;
        }
        Some(letter.to_ascii_uppercase())
    }

    fn to_wide(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }

    fn wide_ptr_to_string(ptr: *const u16) -> String {
        let mut len = 0_usize;
        unsafe {
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        }
    }

    fn device_number_for_path(device_path: &str) -> Option<u32> {
        let wide_path = to_wide(device_path);

        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide_path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .ok()?;

        let mut device_number = STORAGE_DEVICE_NUMBER::default();
        let mut bytes_returned: u32 = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                None,
                0,
                Some(std::ptr::addr_of_mut!(device_number).cast()),
                size_of::<STORAGE_DEVICE_NUMBER>() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };

        unsafe {
            let _ = CloseHandle(handle);
        }

        if ok.is_ok() {
            Some(device_number.DeviceNumber)
        } else {
            None
        }
    }

    fn find_parent_devinst(target_device_number: u32) -> Option<u32> {
        let device_info_set = unsafe {
            SetupDiGetClassDevsW(
                Some(&GUID_DEVINTERFACE_DISK),
                PCWSTR::null(),
                HWND::default(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }
        .ok()?;

        let result = find_matching_devinst(device_info_set, target_device_number);

        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(device_info_set);
        }

        result
    }

    fn find_matching_devinst(device_info_set: HDEVINFO, target_device_number: u32) -> Option<u32> {
        let mut index = 0_u32;

        loop {
            let mut interface_data = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };

            let enumerated = unsafe {
                SetupDiEnumDeviceInterfaces(
                    device_info_set,
                    None,
                    &GUID_DEVINTERFACE_DISK,
                    index,
                    &mut interface_data,
                )
            };

            if enumerated.is_err() {
                return None;
            }

            let mut devinfo_data = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };

            if let Some(device_path) =
                device_interface_path(device_info_set, &interface_data, &mut devinfo_data)
            {
                if device_number_for_path(&device_path) == Some(target_device_number) {
                    let mut parent_devinst = 0_u32;
                    let status =
                        unsafe { CM_Get_Parent(&mut parent_devinst, devinfo_data.DevInst, 0) };
                    if status == 0 {
                        return Some(parent_devinst);
                    }
                    return None;
                }
            }

            index += 1;
        }
    }

    fn device_interface_path(
        device_info_set: HDEVINFO,
        interface_data: &SP_DEVICE_INTERFACE_DATA,
        devinfo_data: &mut SP_DEVINFO_DATA,
    ) -> Option<String> {
        let mut required_size = 0_u32;
        unsafe {
            let _ = SetupDiGetDeviceInterfaceDetailW(
                device_info_set,
                interface_data,
                None,
                0,
                Some(&mut required_size),
                None,
            );
        }

        if required_size == 0 {
            return None;
        }

        let mut buffer = vec![0_u8; required_size as usize];
        let detail_data = buffer
            .as_mut_ptr()
            .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
        unsafe {
            (*detail_data).cbSize = (size_of::<u32>() + size_of::<u16>()) as u32;
        }

        let ok = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info_set,
                interface_data,
                Some(detail_data),
                required_size,
                None,
                Some(devinfo_data),
            )
        };

        if ok.is_err() {
            return None;
        }

        let device_path_ptr = unsafe { (*detail_data).DevicePath.as_ptr() };
        Some(wide_ptr_to_string(device_path_ptr))
    }

    fn request_eject(devinst: u32) -> MenuActionStatus {
        let mut veto_type = PNP_VETO_TYPE::default();
        let mut veto_name = [0_u16; 260];

        let status = unsafe {
            CM_Request_Device_EjectW(devinst, Some(&mut veto_type), Some(&mut veto_name), 0)
        };

        if status == 0 && veto_type == PNP_VETO_TYPE::default() {
            return MenuActionStatus::handled_with_message("ejected");
        }

        let veto_message = wide_ptr_to_string(veto_name.as_ptr());
        let message = if veto_message.is_empty() {
            format!("eject-veto-{}", veto_type.0)
        } else {
            veto_message
        };

        log::warn!("Windows eject request refused: {message}");
        MenuActionStatus {
            handled: false,
            message: Some(message),
        }
    }
}
