#[cfg(all(windows, not(feature = "test-utils")))]
mod platform {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use libloading::Library;

    const EVERYTHING_REQUEST_SIZE: u32 = 0x0000_0010;
    const EVERYTHING_ERROR_IPC: u32 = 2;

    type SetSearchW = unsafe extern "system" fn(*const u16);
    type SetRequestFlags = unsafe extern "system" fn(u32);
    type SetMax = unsafe extern "system" fn(u32);
    type QueryW = unsafe extern "system" fn(bool) -> bool;
    type GetNumResults = unsafe extern "system" fn() -> u32;
    type GetResultSize = unsafe extern "system" fn(u32, *mut i64);
    type GetLastError = unsafe extern "system" fn() -> u32;
    type IsDbLoaded = unsafe extern "system" fn() -> bool;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EverythingAvailability {
        Available,
        NotReady,
        Unavailable,
    }

    #[derive(Debug)]
    pub enum EverythingError {
        DllUnavailable,
        NotReady,
        QueryFailed(u32),
    }

    impl std::fmt::Display for EverythingError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::DllUnavailable => write!(f, "Everything64.dll unavailable"),
                Self::NotReady => write!(f, "Everything database is not ready"),
                Self::QueryFailed(code) => {
                    write!(f, "Everything query failed with error code {code}")
                }
            }
        }
    }

    impl std::error::Error for EverythingError {}

    pub struct EverythingHandle {
        _library: Arc<Library>,
        set_search_w: SetSearchW,
        set_request_flags: SetRequestFlags,
        set_max: SetMax,
        query_w: QueryW,
        get_num_results: GetNumResults,
        get_result_size: GetResultSize,
        get_last_error: GetLastError,
        is_db_loaded: IsDbLoaded,
    }

    impl EverythingHandle {
        pub fn load() -> Result<Self, EverythingError> {
            let path = dll_candidates()
                .into_iter()
                .find(|candidate| candidate.exists())
                .unwrap_or_else(|| PathBuf::from("Everything64.dll"));

            let library =
                unsafe { Library::new(path) }.map_err(|_| EverythingError::DllUnavailable)?;
            let library = Arc::new(library);

            unsafe {
                Ok(Self {
                    set_search_w: *library
                        .get(b"Everything_SetSearchW\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    set_request_flags: *library
                        .get(b"Everything_SetRequestFlags\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    set_max: *library
                        .get(b"Everything_SetMax\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    query_w: *library
                        .get(b"Everything_QueryW\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    get_num_results: *library
                        .get(b"Everything_GetNumResults\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    get_result_size: *library
                        .get(b"Everything_GetResultSize\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    get_last_error: *library
                        .get(b"Everything_GetLastError\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    is_db_loaded: *library
                        .get(b"Everything_IsDBLoaded\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    _library: library,
                })
            }
        }

        pub fn availability(&self) -> EverythingAvailability {
            if unsafe { (self.is_db_loaded)() } {
                EverythingAvailability::Available
            } else {
                EverythingAvailability::NotReady
            }
        }

        pub fn query_folder_size(&self, path: &Path) -> Result<Option<u64>, EverythingError> {
            if self.availability() != EverythingAvailability::Available {
                return Err(EverythingError::NotReady);
            }

            let escaped = path.to_string_lossy().replace('"', "\"\"");
            let query = format!("exact:folder:\"{escaped}\"");
            let wide_query = to_wide(&query);

            unsafe {
                (self.set_search_w)(wide_query.as_ptr());
                (self.set_request_flags)(EVERYTHING_REQUEST_SIZE);
                (self.set_max)(1);

                if !(self.query_w)(true) {
                    let code = (self.get_last_error)();
                    return if code == EVERYTHING_ERROR_IPC {
                        Err(EverythingError::NotReady)
                    } else {
                        Err(EverythingError::QueryFailed(code))
                    };
                }

                if (self.get_num_results)() == 0 {
                    return Ok(None);
                }

                let mut size = 0_i64;
                (self.get_result_size)(0, &mut size as *mut i64);
                Ok((size >= 0).then_some(size as u64))
            }
        }
    }

    fn dll_candidates() -> Vec<PathBuf> {
        let mut candidates = vec![
            PathBuf::from("Everything64.dll"),
            PathBuf::from("resources/windows/Everything64.dll"),
        ];

        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                candidates.push(parent.join("Everything64.dll"));
                candidates.push(
                    parent
                        .join("resources")
                        .join("windows")
                        .join("Everything64.dll"),
                );
            }
        }

        candidates
    }

    fn to_wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}

#[cfg(all(windows, not(feature = "test-utils")))]
pub use platform::*;

// Under `test-utils`, Windows builds must never load the real `Everything64.dll`
// or hit the Everything IPC service. This stub mirrors the Windows API surface so
// `size::mod` compiles identically, but reports the safe unavailable fallback.
#[cfg(all(windows, feature = "test-utils"))]
mod platform {
    use std::path::Path;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EverythingAvailability {
        Available,
        NotReady,
        Unavailable,
    }

    #[derive(Debug)]
    pub enum EverythingError {
        NotReady,
    }

    pub struct EverythingHandle;

    impl EverythingHandle {
        /// The fake loader always succeeds, but the handle reports the service as
        /// unavailable so no test path can ever observe a real Everything result.
        pub fn load() -> Result<Self, EverythingError> {
            Ok(Self)
        }

        pub fn availability(&self) -> EverythingAvailability {
            EverythingAvailability::Unavailable
        }

        pub fn query_folder_size(&self, _path: &Path) -> Result<Option<u64>, EverythingError> {
            Err(EverythingError::NotReady)
        }
    }
}

#[cfg(all(windows, feature = "test-utils"))]
pub use platform::*;

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EverythingAvailability {
        Unavailable,
    }

    #[derive(Debug)]
    pub enum EverythingError {
        Unsupported,
    }

    impl std::fmt::Display for EverythingError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Everything is unsupported on this platform")
        }
    }

    impl std::error::Error for EverythingError {}

    pub struct EverythingHandle;

    impl EverythingHandle {
        pub fn load() -> Result<Self, EverythingError> {
            Err(EverythingError::Unsupported)
        }

        pub fn availability(&self) -> EverythingAvailability {
            EverythingAvailability::Unavailable
        }

        pub fn query_folder_size(&self, _path: &Path) -> Result<Option<u64>, EverythingError> {
            Err(EverythingError::Unsupported)
        }
    }
}

#[cfg(not(windows))]
pub use platform::*;
