use std::path::Path;
use std::sync::OnceLock;

use rayon::{ThreadPool, ThreadPoolBuilder};

const ICON_POOL_THREADS: usize = 4;

pub fn icon_pool() -> Option<&'static ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(ICON_POOL_THREADS)
            .thread_name(|index| format!("fe-icon-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}

pub fn icon_data_url_for_path(path: &Path, is_dir: bool) -> Option<String> {
    if !has_native_icon(path, is_dir) {
        return None;
    }

    #[cfg(all(windows, not(feature = "test-utils")))]
    {
        windows_impl::icon_data_url_for_path(path)
    }

    #[cfg(any(not(windows), feature = "test-utils"))]
    {
        let _ = path;
        None
    }
}

/// Resolves the best available icon for a file: a real per-file extraction
/// first, falling back to the fast per-type icon (resolved from the
/// extension alone, without opening the file) when real extraction yields
/// nothing.
pub fn resolve_icon(path: &Path, is_dir: bool) -> Option<String> {
    if !has_native_icon(path, is_dir) {
        return None;
    }

    if let Some(icon) = icon_data_url_for_path(path, is_dir) {
        return Some(icon);
    }

    #[cfg(all(windows, not(feature = "test-utils")))]
    {
        windows_impl::icon_data_url_for_type(path)
    }

    #[cfg(any(not(windows), feature = "test-utils"))]
    {
        None
    }
}

fn has_native_icon(path: &Path, is_dir: bool) -> bool {
    !is_dir && should_use_native_icon(path)
}

fn should_use_native_icon(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "exe" | "com" | "scr" | "msi" | "msix" | "dll"
    )
}

#[cfg(all(windows, not(feature = "test-utils")))]
mod windows_impl {
    use super::*;
    use std::mem::size_of;

    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES};
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_FLAGS, SHGFI_ICON, SHGFI_SMALLICON,
        SHGFI_USEFILEATTRIBUTES,
    };

    use crate::win32::gdi::{hicon_to_data_url, IconBitmapFallback, IconGuard};
    use crate::win32::wide_nul;

    pub(super) fn icon_data_url_for_path(path: &Path) -> Option<String> {
        small_shell_icon(
            path,
            FILE_FLAGS_AND_ATTRIBUTES::default(),
            SHGFI_ICON | SHGFI_SMALLICON,
        )
    }

    /// Resolves an icon by extension alone (`SHGFI_USEFILEATTRIBUTES`), never
    /// opening the underlying file. Used as a fast fallback when real
    /// per-file extraction fails or is too slow to run inline.
    pub(super) fn icon_data_url_for_type(path: &Path) -> Option<String> {
        small_shell_icon(
            path,
            FILE_ATTRIBUTE_NORMAL,
            SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES,
        )
    }

    fn small_shell_icon(
        path: &Path,
        attributes: FILE_FLAGS_AND_ATTRIBUTES,
        flags: SHGFI_FLAGS,
    ) -> Option<String> {
        let wide_path = wide_nul(path);
        let mut info = SHFILEINFOW::default();
        let result = unsafe {
            SHGetFileInfoW(
                PCWSTR(wide_path.as_ptr()),
                attributes,
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                flags,
            )
        };
        let icon_found = result != 0 && !info.hIcon.is_invalid();
        if !icon_found {
            return None;
        }

        let icon = IconGuard(info.hIcon);
        hicon_to_data_url(icon.0, IconBitmapFallback::Mask)
    }
}
