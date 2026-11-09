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

#[derive(Clone, Copy)]
pub struct TransferPaths<'path> {
    pub source: &'path Path,
    pub target: &'path Path,
}

pub fn rename_or_cross_device<F>(
    paths: TransferPaths<'_>,
    same_resource: bool,
    rename: F,
) -> Result<bool, String>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    if !same_resource {
        return Ok(false);
    }
    match rename(paths.source, paths.target) {
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
        AdapterSelection::Portable(PortableReason::UnsupportedPlatform)
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
