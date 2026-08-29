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

/// `ShellExecuteW` reports failure as a status code of 32 or less.
const SHELL_EXECUTE_SUCCESS_FLOOR: isize = 32;
/// `SE_ERR_ASSOCINCOMPLETE`: the association exists but is unusable.
const SE_ERR_ASSOCINCOMPLETE: isize = 27;
/// `SE_ERR_NOASSOC`: no application is associated with the extension.
const SE_ERR_NOASSOC: isize = 31;

/// `SE_ERR_ACCESSDENIED`: for a "runas" launch this is the refused UAC prompt.
const SE_ERR_ACCESSDENIED: isize = 5;

/// Verb that raises the shell's "Open with" picker.
pub const OPEN_WITH_VERB: &str = "openas";

/// Verb that relaunches a program elevated. The shell raises the UAC prompt.
pub const RUNAS_VERB: &str = "runas";

pub fn shell_execute_succeeded(status: isize) -> bool {
    status > SHELL_EXECUTE_SUCCESS_FLOOR
}

/// Explorer falls back to the "Open with" picker for types it cannot resolve;
/// erroring out instead would leave the user no way to open them at all.
pub fn should_prompt_open_with(status: isize) -> bool {
    matches!(status, SE_ERR_ASSOCINCOMPLETE | SE_ERR_NOASSOC)
}

pub fn launch_failure_detail(status: isize) -> String {
    format!("ShellExecuteW returned status code {status}")
}

/// A refused UAC prompt is a user decision, not a defect. Report it in words
/// instead of a raw shell status code.
pub fn elevation_failure_detail(status: isize) -> String {
    if status == SE_ERR_ACCESSDENIED {
        return "the administrator permission request was refused".to_string();
    }

    launch_failure_detail(status)
}

/// Restarts this application with administrator permissions.
///
/// Windows only. The other platforms have no equivalent one-click elevation,
/// so they report `Unsupported` and the caller tells the user to restart the
/// application manually.
pub fn restart_as_admin() -> Result<(), OpenPathError> {
    let executable = std::env::current_exe().map_err(|source| OpenPathError::Io {
        path: PathBuf::new(),
        source,
    })?;

    platform::restart_as_admin(&executable)
}

/// Matches Explorer-style launches: scripts and associated apps start in the
/// clicked item's containing folder, not the file explorer process directory.
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
}

#[cfg(all(not(feature = "test-utils"), windows))]
mod platform {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::OpenPathError;

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn open_path(path: &Path) -> Result<(), OpenPathError> {
        let target = wide(path.as_os_str());
        let directory = super::launch_directory(path).map(|value| wide(value.as_os_str()));
        let directory_ptr = directory
            .as_ref()
            .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));

        // A null verb makes the shell pick the file type's *default* verb, which
        // is what a double-click in Explorer does. Hard-coding "open" breaks
        // every type whose default verb is something else — `.iso` registers
        // `mount`, so "open" failed with SE_ERR_NOASSOC instead of mounting.
        let status = shell_execute(PCWSTR::null(), &target, directory_ptr);
        if super::shell_execute_succeeded(status) {
            return Ok(());
        }

        if super::should_prompt_open_with(status) {
            let open_with = wide(OsStr::new(super::OPEN_WITH_VERB));
            let fallback = shell_execute(PCWSTR(open_with.as_ptr()), &target, directory_ptr);
            if super::shell_execute_succeeded(fallback) {
                return Ok(());
            }
        }

        Err(OpenPathError::LaunchFailed {
            path: path.to_path_buf(),
            detail: super::launch_failure_detail(status),
        })
    }

    pub fn restart_as_admin(executable: &Path) -> Result<(), OpenPathError> {
        // An elevated process cannot elevate again. Report it instead of
        // closing the window and gaining nothing.
        if unsafe { IsUserAnAdmin() }.as_bool() {
            return Err(OpenPathError::LaunchFailed {
                path: executable.to_path_buf(),
                detail: "the application already runs with administrator permissions".to_string(),
            });
        }

        let verb = wide(OsStr::new(super::RUNAS_VERB));
        let target = wide(executable.as_os_str());
        let directory = super::launch_directory(executable).map(|value| wide(value.as_os_str()));
        let directory_ptr = directory
            .as_ref()
            .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));

        let status = shell_execute(PCWSTR(verb.as_ptr()), &target, directory_ptr);
        if super::shell_execute_succeeded(status) {
            return Ok(());
        }

        Err(OpenPathError::LaunchFailed {
            path: executable.to_path_buf(),
            detail: super::elevation_failure_detail(status),
        })
    }

    fn shell_execute(operation: PCWSTR, target: &[u16], directory: PCWSTR) -> isize {
        let result = unsafe {
            ShellExecuteW(
                None,
                operation,
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                directory,
                SW_SHOWNORMAL,
            )
        };

        result.0 as isize
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

    /// Only Windows has a one-click elevated relaunch. macOS asks the user to
    /// start the application again with the permissions it needs.
    pub fn restart_as_admin(_executable: &Path) -> Result<(), OpenPathError> {
        Err(OpenPathError::Unsupported)
    }
}
