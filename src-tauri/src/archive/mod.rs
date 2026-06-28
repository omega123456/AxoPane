#[cfg(any(
    not(any(target_os = "windows", target_os = "macos")),
    feature = "test-utils"
))]
mod unsupported;
#[cfg(all(
    any(target_os = "windows", target_os = "macos"),
    not(feature = "test-utils")
))]
mod zip_backend;

use crate::ipc::types::{CompressArchiveRequest, ExtractArchiveRequest, MenuActionStatus};

pub fn compress_archive(payload: CompressArchiveRequest) -> MenuActionStatus {
    #[cfg(all(
        any(target_os = "windows", target_os = "macos"),
        not(feature = "test-utils")
    ))]
    {
        return zip_backend::compress_archive(payload);
    }

    #[cfg(any(
        not(any(target_os = "windows", target_os = "macos")),
        feature = "test-utils"
    ))]
    {
        unsupported::compress_archive(payload)
    }
}

pub fn extract_archive(payload: ExtractArchiveRequest) -> MenuActionStatus {
    #[cfg(all(
        any(target_os = "windows", target_os = "macos"),
        not(feature = "test-utils")
    ))]
    {
        return zip_backend::extract_archive(payload);
    }

    #[cfg(any(
        not(any(target_os = "windows", target_os = "macos")),
        feature = "test-utils"
    ))]
    {
        unsupported::extract_archive(payload)
    }
}
