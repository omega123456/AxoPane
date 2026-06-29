use std::collections::HashMap;
use std::path::PathBuf;

pub const EVERYTHING_BATCH_CHUNK_SIZE: usize = 64;

pub fn escape_exact_folder_query_path(path: &str) -> String {
    path.replace('"', "\"\"")
}

pub fn build_exact_folder_query(path: &str) -> String {
    format!("exact:folder:\"{}\"", escape_exact_folder_query_path(path))
}

pub fn build_exact_folder_or_queries(paths: &[String], chunk_size: usize) -> Vec<String> {
    let chunk_size = chunk_size.max(1);

    paths
        .chunks(chunk_size)
        .map(|chunk| {
            chunk
                .iter()
                .map(|path| build_exact_folder_query(path))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect()
}

pub fn normalize_result_path(path: &str) -> String {
    #[cfg(windows)]
    {
        let mut normalized = path.replace('/', "\\");
        let lower = normalized.to_lowercase();
        if lower.starts_with("\\\\?\\unc\\") {
            normalized = format!("\\\\{}", normalized[8..].replace('/', "\\"));
        } else if lower.starts_with("\\\\?\\") {
            normalized = normalized[4..].to_string();
        }

        if !is_windows_root_like(&normalized) {
            normalized = normalized.trim_end_matches(['\\', '/']).to_string();
        }

        normalized.to_lowercase()
    }

    #[cfg(not(windows))]
    {
        if path == "/" {
            return path.to_string();
        }

        path.trim_end_matches('/').to_string()
    }
}

pub fn map_everything_result_sizes(
    requested_paths: &[String],
    results: &[(String, String, Option<u64>)],
) -> HashMap<String, Option<u64>> {
    let mut mapped = requested_paths
        .iter()
        .cloned()
        .map(|path| (path, None))
        .collect::<HashMap<_, _>>();
    let requested_by_normalized = requested_paths
        .iter()
        .map(|path| (normalize_result_path(path), path.clone()))
        .collect::<HashMap<_, _>>();

    for (result_path, file_name, size_bytes) in results {
        let full_path = join_everything_result_path(result_path, file_name);
        let normalized = normalize_result_path(&full_path);
        if let Some(requested_path) = requested_by_normalized.get(&normalized) {
            mapped.insert(requested_path.clone(), *size_bytes);
        }
    }

    mapped
}

pub fn join_everything_result_path(result_path: &str, file_name: &str) -> String {
    #[cfg(windows)]
    {
        if result_path.ends_with(':') {
            return format!(r"{result_path}\{file_name}");
        }
    }

    PathBuf::from(result_path)
        .join(file_name)
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn is_windows_root_like(path: &str) -> bool {
    let trimmed = path.trim_end_matches(['\\', '/']);
    let lower = trimmed.to_lowercase();
    (trimmed.len() == 2 && trimmed.ends_with(':'))
        || (lower.starts_with("\\\\") && lower.matches('\\').count() <= 3)
}

#[cfg(all(windows, not(feature = "test-utils")))]
mod platform {
    use super::{
        build_exact_folder_or_queries, map_everything_result_sizes, EVERYTHING_BATCH_CHUNK_SIZE,
    };
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use libloading::Library;

    const EVERYTHING_REQUEST_FILE_NAME: u32 = 0x0000_0001;
    const EVERYTHING_REQUEST_PATH: u32 = 0x0000_0002;
    const EVERYTHING_REQUEST_SIZE: u32 = 0x0000_0010;
    const EVERYTHING_ERROR_IPC: u32 = 2;

    type SetSearchW = unsafe extern "system" fn(*const u16);
    type SetRequestFlags = unsafe extern "system" fn(u32);
    type SetMax = unsafe extern "system" fn(u32);
    type QueryW = unsafe extern "system" fn(bool) -> bool;
    type GetNumResults = unsafe extern "system" fn() -> u32;
    type GetResultSize = unsafe extern "system" fn(u32, *mut i64) -> bool;
    type GetResultPathW = unsafe extern "system" fn(u32) -> *const u16;
    type GetResultFileNameW = unsafe extern "system" fn(u32) -> *const u16;
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
        query_lock: Mutex<()>,
        set_search_w: SetSearchW,
        set_request_flags: SetRequestFlags,
        set_max: SetMax,
        query_w: QueryW,
        get_num_results: GetNumResults,
        get_result_size: GetResultSize,
        get_result_path_w: GetResultPathW,
        get_result_file_name_w: GetResultFileNameW,
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
                    query_lock: Mutex::new(()),
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
                    get_result_path_w: *library
                        .get(b"Everything_GetResultPathW\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    get_result_file_name_w: *library
                        .get(b"Everything_GetResultFileNameW\0")
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
            let requested = vec![path.to_string_lossy().into_owned()];
            self.query_folder_sizes(&requested)
                .map(|sizes| sizes.get(&requested[0]).copied().flatten())
        }

        pub fn query_folder_sizes(
            &self,
            paths: &[String],
        ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
            if self.availability() != EverythingAvailability::Available {
                return Err(EverythingError::NotReady);
            }

            if paths.is_empty() {
                return Ok(HashMap::new());
            }

            let _query_guard = self.query_lock.lock().expect("everything query lock");
            let mut sizes = paths
                .iter()
                .cloned()
                .map(|path| (path, None))
                .collect::<HashMap<_, _>>();

            for chunk in paths.chunks(EVERYTHING_BATCH_CHUNK_SIZE) {
                let chunk_paths = chunk.to_vec();
                let query =
                    build_exact_folder_or_queries(&chunk_paths, EVERYTHING_BATCH_CHUNK_SIZE)
                        .into_iter()
                        .next()
                        .expect("chunk query");
                let wide_query = to_wide(&query);

                unsafe {
                    (self.set_search_w)(wide_query.as_ptr());
                    (self.set_request_flags)(
                        EVERYTHING_REQUEST_FILE_NAME
                            | EVERYTHING_REQUEST_PATH
                            | EVERYTHING_REQUEST_SIZE,
                    );
                    (self.set_max)(chunk_paths.len() as u32);

                    if !(self.query_w)(true) {
                        let code = (self.get_last_error)();
                        return if code == EVERYTHING_ERROR_IPC {
                            Err(EverythingError::NotReady)
                        } else {
                            Err(EverythingError::QueryFailed(code))
                        };
                    }

                    let result_count = (self.get_num_results)();
                    let mut results = Vec::with_capacity(result_count as usize);
                    for index in 0..result_count {
                        let Some(result_path) = wide_ptr_to_string((self.get_result_path_w)(index))
                        else {
                            continue;
                        };
                        let Some(file_name) =
                            wide_ptr_to_string((self.get_result_file_name_w)(index))
                        else {
                            continue;
                        };

                        let mut size = 0_i64;
                        let size_bytes =
                            if (self.get_result_size)(index, &mut size as *mut i64) && size >= 0 {
                                Some(size as u64)
                            } else {
                                None
                            };
                        results.push((result_path, file_name, size_bytes));
                    }

                    for (path, size_bytes) in map_everything_result_sizes(&chunk_paths, &results) {
                        sizes.insert(path, size_bytes);
                    }
                }
            }

            Ok(sizes)
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

    fn wide_ptr_to_string(value: *const u16) -> Option<String> {
        if value.is_null() {
            return None;
        }

        let mut length = 0;
        unsafe {
            while *value.add(length) != 0 {
                length += 1;
            }
            let slice = std::slice::from_raw_parts(value, length);
            Some(String::from_utf16_lossy(slice))
        }
    }
}

#[cfg(all(windows, not(feature = "test-utils")))]
pub use platform::*;

// Under `test-utils`, Windows builds must never load the real `Everything64.dll`
// or hit the Everything IPC service. This stub mirrors the Windows API surface so
// `size::mod` compiles identically, but reports the safe unavailable fallback.
#[cfg(all(windows, feature = "test-utils"))]
mod platform {
    use std::collections::HashMap;
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

    pub struct EverythingHandle {
        availability: EverythingAvailability,
        results: HashMap<String, Option<u64>>,
        fail_queries: bool,
    }

    impl EverythingHandle {
        /// The fake loader always succeeds, but the handle reports the service as
        /// unavailable so no test path can ever observe a real Everything result.
        pub fn load() -> Result<Self, EverythingError> {
            Ok(Self {
                availability: EverythingAvailability::Unavailable,
                results: HashMap::new(),
                fail_queries: false,
            })
        }

        pub fn test_available(results: HashMap<String, Option<u64>>) -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results,
                fail_queries: false,
            }
        }

        pub fn test_available_error() -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results: HashMap::new(),
                fail_queries: true,
            }
        }

        pub fn availability(&self) -> EverythingAvailability {
            self.availability.clone()
        }

        pub fn query_folder_size(&self, _path: &Path) -> Result<Option<u64>, EverythingError> {
            self.query_folder_sizes(&[_path.to_string_lossy().into_owned()])
                .map(|sizes| sizes.values().next().copied().flatten())
        }

        pub fn query_folder_sizes(
            &self,
            paths: &[String],
        ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
            if self.availability != EverythingAvailability::Available || self.fail_queries {
                return Err(EverythingError::NotReady);
            }

            Ok(paths
                .iter()
                .cloned()
                .map(|path| {
                    let size_bytes = self.results.get(&path).copied().flatten();
                    (path, size_bytes)
                })
                .collect())
        }
    }
}

#[cfg(all(windows, feature = "test-utils"))]
pub use platform::*;

// Non-Windows targets never have Everything. This stub mirrors the Windows API
// surface (including the unused `Available` / `NotReady` variants) so `size::mod`
// compiles identically across platforms; it always reports the unavailable
// fallback. The variants are `pub`, so they raise no dead-code warnings.
#[cfg(not(windows))]
mod platform {
    use std::collections::HashMap;
    use std::path::Path;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EverythingAvailability {
        Available,
        NotReady,
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

    pub struct EverythingHandle {
        availability: EverythingAvailability,
        results: HashMap<String, Option<u64>>,
        fail_queries: bool,
    }

    impl EverythingHandle {
        pub fn load() -> Result<Self, EverythingError> {
            Err(EverythingError::Unsupported)
        }

        pub fn test_available(results: HashMap<String, Option<u64>>) -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results,
                fail_queries: false,
            }
        }

        pub fn test_available_error() -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results: HashMap::new(),
                fail_queries: true,
            }
        }

        pub fn availability(&self) -> EverythingAvailability {
            self.availability.clone()
        }

        pub fn query_folder_size(&self, _path: &Path) -> Result<Option<u64>, EverythingError> {
            self.query_folder_sizes(&[_path.to_string_lossy().into_owned()])
                .map(|sizes| sizes.values().next().copied().flatten())
        }

        pub fn query_folder_sizes(
            &self,
            paths: &[String],
        ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
            if self.availability != EverythingAvailability::Available || self.fail_queries {
                return Err(EverythingError::Unsupported);
            }

            Ok(paths
                .iter()
                .cloned()
                .map(|path| {
                    let size_bytes = self.results.get(&path).copied().flatten();
                    (path, size_bytes)
                })
                .collect())
        }
    }
}

#[cfg(not(windows))]
pub use platform::*;
