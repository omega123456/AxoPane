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

const SHELL_EXECUTE_SUCCESS_FLOOR: isize = 32;
const SE_ERR_ASSOCINCOMPLETE: isize = 27;
const SE_ERR_NOASSOC: isize = 31;

const SE_ERR_ACCESSDENIED: isize = 5;

pub const OPEN_WITH_VERB: &str = "openas";

pub const RUNAS_VERB: &str = "runas";

pub fn shell_execute_succeeded(status: isize) -> bool {
    status > SHELL_EXECUTE_SUCCESS_FLOOR
}

pub fn should_prompt_open_with(status: isize) -> bool {
    matches!(status, SE_ERR_ASSOCINCOMPLETE | SE_ERR_NOASSOC)
}

pub fn launch_failure_detail(status: isize) -> String {
    format!("ShellExecuteW returned status code {status}")
}

pub fn elevation_failure_detail(status: isize) -> String {
    if status == SE_ERR_ACCESSDENIED {
        return "the administrator permission request was refused".to_string();
    }

    launch_failure_detail(status)
}

pub fn restart_as_admin() -> Result<(), OpenPathError> {
    let executable = std::env::current_exe().map_err(|source| OpenPathError::Io {
        path: PathBuf::new(),
        source,
    })?;

    platform::restart_as_admin(&executable)
}

pub const FULL_DISK_ACCESS_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

pub fn open_full_disk_access_settings() -> Result<(), OpenPathError> {
    platform::open_full_disk_access_settings()
}

pub fn launch_directory(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

#[cfg(feature = "test-utils")]
mod platform {
    use std::path::Path;

    use super::OpenPathError;

    pub fn open_path(_path: &Path) -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
    }

    pub fn restart_as_admin(_executable: &Path) -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
    }

    pub fn open_full_disk_access_settings() -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
mod platform {
    use std::ffi::OsStr;
    use std::path::Path;

    use windows::Win32::UI::Shell::IsUserAnAdmin;

    use super::OpenPathError;
    use crate::win32::shell::{shell_execute, ShellExecuteRequest};

    pub fn open_path(path: &Path) -> Result<(), OpenPathError> {
        let directory = super::launch_directory(path);
        let open_default = ShellExecuteRequest {
            verb: None,
            file: path.as_os_str(),
            parameters: None,
            directory: directory.as_deref().map(Path::as_os_str),
        };

        let status = shell_execute(&open_default);
        if super::shell_execute_succeeded(status) {
            return Ok(());
        }

        let opened_with_prompt =
            super::should_prompt_open_with(status) && open_with_prompt(open_default);
        if opened_with_prompt {
            return Ok(());
        }

        Err(OpenPathError::LaunchFailed {
            path: path.to_path_buf(),
            detail: super::launch_failure_detail(status),
        })
    }

    fn open_with_prompt(open_default: ShellExecuteRequest<'_>) -> bool {
        let fallback = shell_execute(&ShellExecuteRequest {
            verb: Some(OsStr::new(super::OPEN_WITH_VERB)),
            ..open_default
        });
        super::shell_execute_succeeded(fallback)
    }

    pub fn restart_as_admin(executable: &Path) -> Result<(), OpenPathError> {
        if unsafe { IsUserAnAdmin() }.as_bool() {
            return Err(OpenPathError::LaunchFailed {
                path: executable.to_path_buf(),
                detail: "the application already runs with administrator permissions".to_string(),
            });
        }

        let directory = super::launch_directory(executable);
        let status = shell_execute(&ShellExecuteRequest {
            verb: Some(OsStr::new(super::RUNAS_VERB)),
            file: executable.as_os_str(),
            parameters: None,
            directory: directory.as_deref().map(Path::as_os_str),
        });
        if super::shell_execute_succeeded(status) {
            return Ok(());
        }

        Err(OpenPathError::LaunchFailed {
            path: executable.to_path_buf(),
            detail: super::elevation_failure_detail(status),
        })
    }

    pub fn open_full_disk_access_settings() -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
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

    pub fn restart_as_admin(_executable: &Path) -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
    }

    #[cfg(target_os = "macos")]
    pub fn open_full_disk_access_settings() -> Result<(), OpenPathError> {
        Command::new(OPEN_COMMAND)
            .arg(super::FULL_DISK_ACCESS_SETTINGS_URL)
            .spawn()
            .map(|_| ())
            .map_err(|source| OpenPathError::Io {
                path: super::FULL_DISK_ACCESS_SETTINGS_URL.into(),
                source,
            })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn open_full_disk_access_settings() -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
    }
}
