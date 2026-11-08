//! Safe-remove ("eject") for removable volumes.
//!
//! This is a machine-global operation (it can physically power down a device) so
//! it must never touch a real OS API under `test-utils`.
//!
//! Eject is **macOS-only**. On Windows a reliable *safe* removal isn't possible
//! from here — a volume that is still in use can only be force-dismounted (with
//! possible data loss), so Windows users are directed to the native "Eject" entry
//! already surfaced in the shell context menu, which offers that choice safely.
//! The command therefore reports `Unsupported` on Windows.

use crate::ipc::types::MenuActionStatus;

#[cfg(all(target_os = "macos", not(feature = "test-utils")))]
pub fn eject_volume(mount_root: &str) -> MenuActionStatus {
    macos::eject_volume(mount_root)
}

#[cfg(any(not(target_os = "macos"), feature = "test-utils"))]
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

    fn eject_failure(mount_root: &str, stderr: &[u8]) -> MenuActionStatus {
        let message = eject_failure_message(stderr);
        log::warn!("diskutil eject failed for {mount_root}: {message}");
        MenuActionStatus {
            handled: false,
            message: Some(message),
        }
    }

    fn eject_failure_message(stderr: &[u8]) -> String {
        let stderr = String::from_utf8_lossy(stderr).trim().to_string();
        if stderr.is_empty() {
            return "eject-failed".to_string();
        }
        stderr
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
            Ok(output) => eject_failure(mount_root, &output.stderr),
            Err(error) => {
                log::warn!("diskutil eject spawn failed for {mount_root}: {error}");
                MenuActionStatus::unsupported("eject-launch-failed")
            }
        }
    }
}
