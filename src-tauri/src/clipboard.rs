use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMode {
    Copy,
    Move,
}

#[derive(Debug)]
pub struct ClipboardError {
    message: String,
}

impl ClipboardError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ClipboardError {}

pub fn write_paths(mode: ClipboardMode, paths: &[String]) -> Result<(), ClipboardError> {
    if paths.is_empty() {
        return clear();
    }

    if paths.iter().any(|path| path.trim().is_empty()) {
        return Err(ClipboardError::new("clipboard paths must not be empty"));
    }

    #[cfg(all(windows, not(feature = "test-utils")))]
    {
        return windows_impl::write_paths(mode, paths);
    }

    #[cfg(all(not(windows), not(feature = "test-utils")))]
    {
        return cross_platform_impl::write_paths(mode, paths);
    }

    #[cfg(feature = "test-utils")]
    {
        let _ = (mode, paths);
        Ok(())
    }
}

pub fn clear() -> Result<(), ClipboardError> {
    #[cfg(all(windows, not(feature = "test-utils")))]
    {
        return windows_impl::clear();
    }

    #[cfg(all(not(windows), not(feature = "test-utils")))]
    {
        return cross_platform_impl::clear();
    }

    #[cfg(feature = "test-utils")]
    {
        Ok(())
    }
}

#[cfg(all(not(windows), not(feature = "test-utils")))]
mod cross_platform_impl {
    use super::{ClipboardError, ClipboardMode};
    use clipboard_rs::{Clipboard, ClipboardContext};

    pub(super) fn write_paths(mode: ClipboardMode, paths: &[String]) -> Result<(), ClipboardError> {
        let _ = mode;

        // macOS/Linux file clipboards are represented as file lists. The app's
        // own clipboard store still tracks move-vs-copy so cut highlighting and
        // in-app paste behavior remain correct even where the OS clipboard does
        // not expose a separate move hint the same way Windows does.
        ClipboardContext::new()
            .map_err(clipboard_error)?
            .set_files(paths.to_vec())
            .map_err(clipboard_error)
    }

    pub(super) fn clear() -> Result<(), ClipboardError> {
        ClipboardContext::new()
            .map_err(clipboard_error)?
            .clear()
            .map_err(clipboard_error)
    }

    fn clipboard_error(error: Box<dyn std::error::Error>) -> ClipboardError {
        ClipboardError::new(error.to_string())
    }
}

#[cfg(all(windows, not(feature = "test-utils")))]
mod windows_impl {
    use super::{ClipboardError, ClipboardMode};
    use std::mem::size_of;
    use std::ptr::{copy_nonoverlapping, null_mut};

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_HDROP;
    use windows::Win32::UI::Shell::DROPFILES;

    const DROPEFFECT_COPY: u32 = 1;
    const DROPEFFECT_MOVE: u32 = 2;

    pub(super) fn write_paths(mode: ClipboardMode, paths: &[String]) -> Result<(), ClipboardError> {
        let clipboard = ClipboardSession::open()?;
        clipboard.empty()?;

        let dropfiles_handle = build_dropfiles_handle(paths)?;
        let preferred_effect_handle = build_preferred_effect_handle(mode)?;

        clipboard
            .set_data(CF_HDROP.0 as u32, dropfiles_handle)
            .map_err(|error| {
                let _ = unsafe { GlobalFree(Some(dropfiles_handle)) };
                error
            })?;

        let preferred_effect_format = register_preferred_drop_effect()?;
        clipboard
            .set_data(preferred_effect_format, preferred_effect_handle)
            .map_err(|error| {
                let _ = unsafe { GlobalFree(Some(preferred_effect_handle)) };
                error
            })?;

        Ok(())
    }

    pub(super) fn clear() -> Result<(), ClipboardError> {
        let clipboard = ClipboardSession::open()?;
        clipboard.empty()
    }

    struct ClipboardSession;

    impl ClipboardSession {
        fn open() -> Result<Self, ClipboardError> {
            unsafe { OpenClipboard(Some(HWND(null_mut()))) }
                .map_err(windows_error)
                .map(|_| Self)
        }

        fn empty(&self) -> Result<(), ClipboardError> {
            unsafe { EmptyClipboard() }.map_err(windows_error)
        }

        fn set_data(&self, format: u32, handle: HGLOBAL) -> Result<(), ClipboardError> {
            unsafe { SetClipboardData(format, Some(HANDLE(handle.0))) }
                .map_err(windows_error)
                .map(|_| ())
        }
    }

    impl Drop for ClipboardSession {
        fn drop(&mut self) {
            let _ = unsafe { CloseClipboard() };
        }
    }

    fn register_preferred_drop_effect() -> Result<u32, ClipboardError> {
        let format_name = wide("Preferred DropEffect");
        let format = unsafe { RegisterClipboardFormatW(PCWSTR(format_name.as_ptr())) };
        if format == 0 {
            return Err(windows_error(windows::core::Error::from_win32()));
        }

        Ok(format)
    }

    fn build_preferred_effect_handle(mode: ClipboardMode) -> Result<HGLOBAL, ClipboardError> {
        let effect = match mode {
            ClipboardMode::Copy => DROPEFFECT_COPY,
            ClipboardMode::Move => DROPEFFECT_MOVE,
        };
        build_handle_from_bytes(&effect.to_le_bytes())
    }

    fn build_dropfiles_handle(paths: &[String]) -> Result<HGLOBAL, ClipboardError> {
        let mut wide_paths = Vec::new();
        for path in paths {
            if path.trim().is_empty() {
                continue;
            }

            wide_paths.extend(path.encode_utf16());
            wide_paths.push(0);
        }
        wide_paths.push(0);

        let payload_bytes_len = wide_paths
            .len()
            .checked_mul(size_of::<u16>())
            .ok_or_else(|| ClipboardError::new("clipboard payload too large"))?;
        let total_bytes = size_of::<DROPFILES>()
            .checked_add(payload_bytes_len)
            .ok_or_else(|| ClipboardError::new("clipboard payload too large"))?;

        let handle = alloc_handle(total_bytes)?;
        let lock = GlobalLockGuard::new(handle)?;

        let header = DROPFILES {
            pFiles: size_of::<DROPFILES>() as u32,
            pt: Default::default(),
            fNC: false.into(),
            fWide: true.into(),
        };

        unsafe {
            copy_nonoverlapping(
                (&header as *const DROPFILES).cast::<u8>(),
                lock.ptr.cast::<u8>(),
                size_of::<DROPFILES>(),
            );

            copy_nonoverlapping(
                wide_paths.as_ptr().cast::<u8>(),
                lock.ptr.cast::<u8>().add(size_of::<DROPFILES>()),
                payload_bytes_len,
            );
        }

        Ok(handle)
    }

    fn build_handle_from_bytes(bytes: &[u8]) -> Result<HGLOBAL, ClipboardError> {
        let handle = alloc_handle(bytes.len())?;
        let lock = GlobalLockGuard::new(handle)?;

        unsafe {
            copy_nonoverlapping(bytes.as_ptr(), lock.ptr.cast::<u8>(), bytes.len());
        }

        Ok(handle)
    }

    fn alloc_handle(len: usize) -> Result<HGLOBAL, ClipboardError> {
        unsafe { GlobalAlloc(GMEM_MOVEABLE, len) }.map_err(windows_error)
    }

    struct GlobalLockGuard {
        handle: HGLOBAL,
        ptr: *mut std::ffi::c_void,
    }

    impl GlobalLockGuard {
        fn new(handle: HGLOBAL) -> Result<Self, ClipboardError> {
            let ptr = unsafe { GlobalLock(handle) };
            if ptr.is_null() {
                return Err(windows_error(windows::core::Error::from_win32()));
            }

            Ok(Self { handle, ptr })
        }
    }

    impl Drop for GlobalLockGuard {
        fn drop(&mut self) {
            let _ = unsafe { GlobalUnlock(self.handle) };
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn windows_error(error: windows::core::Error) -> ClipboardError {
        ClipboardError::new(error.to_string())
    }
}
