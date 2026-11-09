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

pub struct ClipboardFiles {
    pub mode: ClipboardMode,
    pub paths: Vec<String>,
}

impl ClipboardFiles {
    fn empty() -> Self {
        Self {
            mode: ClipboardMode::Copy,
            paths: Vec::new(),
        }
    }
}

pub fn write_paths(mode: ClipboardMode, paths: &[String]) -> Result<(), ClipboardError> {
    if paths.is_empty() {
        return clear();
    }

    if paths.iter().any(|path| path.trim().is_empty()) {
        return Err(ClipboardError::new("clipboard paths must not be empty"));
    }

    #[cfg(all(windows, not(feature = "test-utils")))]
    {
        windows_impl::write_paths(mode, paths)
    }

    #[cfg(all(not(windows), not(feature = "test-utils")))]
    {
        cross_platform_impl::write_paths(mode, paths)
    }

    #[cfg(feature = "test-utils")]
    {
        let _ = (mode, paths);
        Ok(())
    }
}

/// Read the file paths another application (or this one) put on the OS
/// clipboard. An empty list means that the clipboard holds no files.
pub fn read_paths() -> Result<ClipboardFiles, ClipboardError> {
    #[cfg(not(feature = "test-utils"))]
    {
        read_impl::read_paths()
    }

    #[cfg(feature = "test-utils")]
    {
        Ok(ClipboardFiles::empty())
    }
}

pub fn clear() -> Result<(), ClipboardError> {
    #[cfg(all(windows, not(feature = "test-utils")))]
    {
        windows_impl::clear()
    }

    #[cfg(all(not(windows), not(feature = "test-utils")))]
    {
        cross_platform_impl::clear()
    }

    #[cfg(feature = "test-utils")]
    {
        Ok(())
    }
}

#[cfg(not(feature = "test-utils"))]
mod read_impl {
    use super::{ClipboardError, ClipboardFiles, ClipboardMode};
    use clipboard_rs::{Clipboard, ClipboardContext};

    pub(super) fn read_paths() -> Result<ClipboardFiles, ClipboardError> {
        let context =
            ClipboardContext::new().map_err(|error| ClipboardError::new(error.to_string()))?;

        // Every platform reports "the clipboard holds no files" as an error.
        // That is an empty clipboard, not a failure.
        let paths = context.get_files().unwrap_or_default();
        if paths.is_empty() {
            return Ok(ClipboardFiles::empty());
        }

        Ok(ClipboardFiles {
            mode: read_mode(),
            paths,
        })
    }

    #[cfg(windows)]
    fn read_mode() -> ClipboardMode {
        super::windows_impl::read_preferred_effect()
    }

    // macOS and Linux file clipboards carry no move hint. A cut inside the app
    // keeps its own mode in the app clipboard store.
    #[cfg(not(windows))]
    fn read_mode() -> ClipboardMode {
        ClipboardMode::Copy
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

    fn clipboard_error(error: Box<dyn std::error::Error + Send + Sync>) -> ClipboardError {
        ClipboardError::new(error.to_string())
    }
}

#[cfg(all(windows, not(feature = "test-utils")))]
#[path = "clipboard/windows_impl.rs"]
mod windows_impl;
