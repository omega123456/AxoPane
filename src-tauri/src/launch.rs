use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum OpenPathError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Unsupported,
    LaunchFailed {
        path: PathBuf,
        detail: String,
    },
}

impl std::fmt::Display for OpenPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { source, .. } => write!(f, "{source}"),
            Self::Unsupported => write!(f, "opening paths is unsupported in this build"),
            Self::LaunchFailed { detail, .. } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for OpenPathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Unsupported | Self::LaunchFailed { .. } => None,
        }
    }
}

pub fn open_path(path: &Path) -> Result<(), OpenPathError> {
    let resolved = dunce::canonicalize(path).map_err(|source| OpenPathError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    platform::open_path(&resolved)
}

#[cfg(feature = "test-utils")]
mod platform {
    use std::path::Path;

    use super::OpenPathError;

    pub fn open_path(_path: &Path) -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
mod platform {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::OpenPathError;

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn open_path(path: &Path) -> Result<(), OpenPathError> {
        let operation = wide(OsStr::new("open"));
        let target = wide(path.as_os_str());

        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };

        let status = result.0 as isize;
        if status <= 32 {
            return Err(OpenPathError::LaunchFailed {
                path: path.to_path_buf(),
                detail: format!("ShellExecuteW returned status code {status}"),
            });
        }

        Ok(())
    }
}

#[cfg(all(not(feature = "test-utils"), not(windows)))]
mod platform {
    use std::path::Path;
    use std::process::Command;

    use super::OpenPathError;

    #[cfg(target_os = "macos")]
    const OPEN_COMMAND: &str = "open";
    #[cfg(not(target_os = "macos"))]
    const OPEN_COMMAND: &str = "xdg-open";

    pub fn open_path(path: &Path) -> Result<(), OpenPathError> {
        Command::new(OPEN_COMMAND)
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|source| OpenPathError::Io {
                path: path.to_path_buf(),
                source,
            })
    }
}
