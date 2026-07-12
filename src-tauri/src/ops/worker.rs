//! Transfer-worker planning helpers kept separate from the queue shell.

use std::path::Path;

#[cfg(feature = "test-utils")]
use super::transfer::PortableReason;
use super::transfer::{AdapterSelection, TransferAdapter, TransferRequirements};
use crate::resource_coordinator::{JobClass, JobSpec};

pub fn transfer_admission(resources: impl IntoIterator<Item = String>, cpu: bool) -> JobSpec {
    let mut classes = vec![JobClass::Throughput];
    if cpu {
        classes.push(JobClass::Cpu);
    }
    JobSpec::new(classes, resources)
}

/// `rename` is attempted only for a known same resource. A non-cross-device
/// error must be surfaced; treating permissions/conflicts as cross-volume can
/// otherwise overwrite source data through an unnecessary fallback.
pub fn rename_or_cross_device<F>(
    source: &Path,
    target: &Path,
    same_resource: bool,
    rename: F,
) -> Result<bool, String>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    if !same_resource {
        return Ok(false);
    }
    match rename(source, target) {
        Ok(()) => Ok(true),
        Err(error) if is_cross_device(&error) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

pub fn select_adapter(
    adapter: &dyn TransferAdapter,
    requirements: TransferRequirements,
) -> AdapterSelection {
    #[cfg(feature = "test-utils")]
    {
        let _ = (adapter, requirements);
        return AdapterSelection::Portable(PortableReason::UnsupportedPlatform);
    }
    #[cfg(not(feature = "test-utils"))]
    {
        adapter.select(requirements)
    }
}

pub fn is_cross_device(error: &std::io::Error) -> bool {
    let raw_os_error = error.raw_os_error();

    #[cfg(windows)]
    if raw_os_error == Some(windows_sys::Win32::Foundation::ERROR_NOT_SAME_DEVICE as i32) {
        return true;
    }

    raw_os_error == Some(libc::EXDEV)
}
