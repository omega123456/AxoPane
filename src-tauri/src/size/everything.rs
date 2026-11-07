use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub const EVERYTHING_BATCH_CHUNK_SIZE: usize = 64;

/// Result orders the SDK can apply before `max` truncates a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EverythingSort {
    NameAscending,
    DateModifiedDescending,
}

impl EverythingSort {
    /// The SDK's `EVERYTHING_SORT_*` value.
    pub fn code(self) -> u32 {
        match self {
            Self::NameAscending => EVERYTHING_SORT_NAME_ASCENDING,
            Self::DateModifiedDescending => EVERYTHING_SORT_DATE_MODIFIED_DESCENDING,
        }
    }
}

/// One Everything query. Every field is applied on every call, so no SDK
/// state leaks between search and folder-size queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EverythingQuery {
    pub search: String,
    pub sort: EverythingSort,
    pub offset: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EverythingRow {
    /// The containing folder, as Everything reports it (`C:` for a drive).
    pub path: String,
    pub file_name: String,
    pub is_folder: bool,
    pub size: Option<u64>,
    /// Windows FILETIME: 100 ns intervals since 1601-01-01 UTC.
    pub modified_filetime: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EverythingQueryResult {
    pub rows: Vec<EverythingRow>,
    /// Matches before `offset` and `max` were applied.
    pub total: u32,
}

/// FILETIME ticks between 1601-01-01 and the Unix epoch.
const FILETIME_UNIX_EPOCH: u64 = 116_444_736_000_000_000;
const NANOS_PER_FILETIME_TICK: u64 = 100;
const EVERYTHING_SORT_NAME_ASCENDING: u32 = 1;
const EVERYTHING_SORT_DATE_MODIFIED_DESCENDING: u32 = 14;

/// Converts a FILETIME; `0` and `u64::MAX` mean "unknown".
pub fn filetime_to_system_time(filetime: u64) -> Option<SystemTime> {
    if filetime == 0 || filetime == u64::MAX {
        return None;
    }
    match filetime.checked_sub(FILETIME_UNIX_EPOCH) {
        Some(since) => SystemTime::UNIX_EPOCH.checked_add(Duration::from_nanos(
            since.saturating_mul(NANOS_PER_FILETIME_TICK),
        )),
        None => SystemTime::UNIX_EPOCH.checked_sub(Duration::from_nanos(
            (FILETIME_UNIX_EPOCH - filetime).saturating_mul(NANOS_PER_FILETIME_TICK),
        )),
    }
}

/// Canned search results for the in-memory handles. `None` fails the query.
#[cfg(any(not(windows), feature = "test-utils"))]
pub type EverythingResponder =
    std::sync::Arc<dyn Fn(&EverythingQuery) -> Option<EverythingQueryResult> + Send + Sync>;

/// The search half of the in-memory handles: records each query and answers
/// it from an optional responder.
#[cfg(any(not(windows), feature = "test-utils"))]
#[derive(Default)]
struct FakeSearch {
    responder: Option<EverythingResponder>,
    recorded: std::sync::Mutex<Vec<EverythingQuery>>,
}

#[cfg(any(not(windows), feature = "test-utils"))]
impl FakeSearch {
    fn with(responder: EverythingResponder) -> Self {
        Self {
            responder: Some(responder),
            recorded: Default::default(),
        }
    }

    fn run(&self, query: &EverythingQuery) -> Option<EverythingQueryResult> {
        self.recorded
            .lock()
            .expect("recorded queries")
            .push(query.clone());
        match &self.responder {
            Some(respond) => respond(query),
            None => Some(EverythingQueryResult::default()),
        }
    }

    fn recorded(&self) -> Vec<EverythingQuery> {
        self.recorded.lock().expect("recorded queries").clone()
    }
}

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
        let mut normalized = crate::path_text::strip_verbatim_prefix(&path.replace('/', "\\"));

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

/// `\\server\share` holds three separators; anything deeper is a subfolder.
#[cfg(windows)]
const UNC_SHARE_ROOT_MAX_SEPARATORS: usize = 3;

#[cfg(windows)]
fn is_windows_root_like(path: &str) -> bool {
    let trimmed = path.trim_end_matches(['\\', '/']);
    crate::path_text::is_drive_designator(trimmed)
        || (trimmed.starts_with(crate::path_text::UNC_PREFIX)
            && trimmed.matches('\\').count() <= UNC_SHARE_ROOT_MAX_SEPARATORS)
}

#[cfg(all(windows, not(feature = "test-utils")))]
mod platform {
    use super::{
        build_exact_folder_or_queries, map_everything_result_sizes, EverythingQuery,
        EverythingQueryResult, EverythingRow, EverythingSort, EVERYTHING_BATCH_CHUNK_SIZE,
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
    const EVERYTHING_REQUEST_DATE_MODIFIED: u32 = 0x0000_0040;
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
    type SetFlag = unsafe extern "system" fn(i32);
    type SetDword = unsafe extern "system" fn(u32);
    type GetTotResults = unsafe extern "system" fn() -> u32;
    type IsFolderResult = unsafe extern "system" fn(u32) -> i32;
    type GetResultDateModified = unsafe extern "system" fn(u32, *mut u64) -> i32;

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
        set_match_path: SetFlag,
        set_match_case: SetFlag,
        set_match_whole_word: SetFlag,
        set_regex: SetFlag,
        set_sort: SetDword,
        set_offset: SetDword,
        get_tot_results: GetTotResults,
        is_folder_result: IsFolderResult,
        get_result_date_modified: GetResultDateModified,
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
                    set_match_path: *library
                        .get(b"Everything_SetMatchPath\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    set_match_case: *library
                        .get(b"Everything_SetMatchCase\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    set_match_whole_word: *library
                        .get(b"Everything_SetMatchWholeWord\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    set_regex: *library
                        .get(b"Everything_SetRegex\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    set_sort: *library
                        .get(b"Everything_SetSort\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    set_offset: *library
                        .get(b"Everything_SetOffset\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    get_tot_results: *library
                        .get(b"Everything_GetTotResults\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    is_folder_result: *library
                        .get(b"Everything_IsFolderResult\0")
                        .map_err(|_| EverythingError::DllUnavailable)?,
                    get_result_date_modified: *library
                        .get(b"Everything_GetResultDateModified\0")
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
            self.query_folder_sizes(&requested, &|| false)
                .map(|sizes| sizes.get(&requested[0]).copied().flatten())
        }

        /// Queries folder sizes for `paths` in bounded chunks. `should_cancel`
        /// is checked before each chunk, so a batch abandoned mid-flight (e.g.
        /// the pane navigated away from a folder with tens of thousands of
        /// subfolders) stops issuing further Everything IPC queries instead of
        /// grinding through every remaining chunk. Any unqueried paths are left
        /// as `None`; the caller discards cancelled results anyway.
        pub fn query_folder_sizes(
            &self,
            paths: &[String],
            should_cancel: &dyn Fn() -> bool,
        ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
            if self.availability() != EverythingAvailability::Available {
                return Err(EverythingError::NotReady);
            }

            if paths.is_empty() {
                return Ok(HashMap::new());
            }

            let mut sizes = paths
                .iter()
                .cloned()
                .map(|path| (path, None))
                .collect::<HashMap<_, _>>();

            for chunk in paths.chunks(EVERYTHING_BATCH_CHUNK_SIZE) {
                if should_cancel() {
                    break;
                }
                let chunk_paths = chunk.to_vec();
                let search =
                    build_exact_folder_or_queries(&chunk_paths, EVERYTHING_BATCH_CHUNK_SIZE)
                        .into_iter()
                        .next()
                        .expect("chunk query");
                let result = self.execute(
                    &EverythingQuery {
                        search,
                        sort: EverythingSort::NameAscending,
                        offset: 0,
                        max: chunk_paths.len() as u32,
                    },
                    EVERYTHING_REQUEST_FILE_NAME
                        | EVERYTHING_REQUEST_PATH
                        | EVERYTHING_REQUEST_SIZE,
                )?;
                let results: Vec<(String, String, Option<u64>)> = result
                    .rows
                    .into_iter()
                    .map(|row| (row.path, row.file_name, row.size))
                    .collect();
                for (path, size_bytes) in map_everything_result_sizes(&chunk_paths, &results) {
                    sizes.insert(path, size_bytes);
                }
            }

            Ok(sizes)
        }

        /// Runs one search query (names, paths, folder flags, modified times).
        pub fn run_query(
            &self,
            query: &EverythingQuery,
        ) -> Result<EverythingQueryResult, EverythingError> {
            if self.availability() != EverythingAvailability::Available {
                return Err(EverythingError::NotReady);
            }
            self.execute(
                query,
                EVERYTHING_REQUEST_FILE_NAME
                    | EVERYTHING_REQUEST_PATH
                    | EVERYTHING_REQUEST_DATE_MODIFIED,
            )
        }

        /// The SDK keeps one process-wide query state, so each call sets all
        /// of it under the lock: a search must never leave match-path, sort or
        /// offset settings behind for a folder-size query, or the reverse.
        fn execute(
            &self,
            query: &EverythingQuery,
            flags: u32,
        ) -> Result<EverythingQueryResult, EverythingError> {
            let wide_search = to_wide(&query.search);
            let _query_guard = self.query_lock.lock().expect("everything query lock");
            unsafe {
                (self.set_search_w)(wide_search.as_ptr());
                (self.set_match_path)(0);
                (self.set_match_case)(0);
                (self.set_match_whole_word)(0);
                (self.set_regex)(0);
                (self.set_sort)(query.sort.code());
                (self.set_offset)(query.offset);
                (self.set_max)(query.max);
                (self.set_request_flags)(flags);

                if !(self.query_w)(true) {
                    let code = (self.get_last_error)();
                    return if code == EVERYTHING_ERROR_IPC {
                        Err(EverythingError::NotReady)
                    } else {
                        Err(EverythingError::QueryFailed(code))
                    };
                }

                let result_count = (self.get_num_results)();
                let mut rows = Vec::with_capacity(result_count as usize);
                for index in 0..result_count {
                    let Some(path) = wide_ptr_to_string((self.get_result_path_w)(index)) else {
                        continue;
                    };
                    let Some(file_name) = wide_ptr_to_string((self.get_result_file_name_w)(index))
                    else {
                        continue;
                    };
                    let mut size = 0_i64;
                    let size = (flags & EVERYTHING_REQUEST_SIZE != 0
                        && (self.get_result_size)(index, &mut size as *mut i64)
                        && size >= 0)
                        .then_some(size as u64);
                    let mut modified = 0_u64;
                    let modified_filetime = (flags & EVERYTHING_REQUEST_DATE_MODIFIED != 0
                        && (self.get_result_date_modified)(index, &mut modified as *mut u64) != 0)
                        .then_some(modified);
                    rows.push(EverythingRow {
                        path,
                        file_name,
                        is_folder: (self.is_folder_result)(index) != 0,
                        size,
                        modified_filetime,
                    });
                }
                Ok(EverythingQueryResult {
                    rows,
                    total: (self.get_tot_results)(),
                })
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
    use super::{EverythingQuery, EverythingQueryResult, EverythingResponder, FakeSearch};
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
        search: FakeSearch,
    }

    impl EverythingHandle {
        /// The fake loader always succeeds, but the handle reports the service as
        /// unavailable so no test path can ever observe a real Everything result.
        pub fn load() -> Result<Self, EverythingError> {
            Ok(Self {
                availability: EverythingAvailability::Unavailable,
                results: HashMap::new(),
                fail_queries: false,
                search: FakeSearch::default(),
            })
        }

        pub fn test_available(results: HashMap<String, Option<u64>>) -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results,
                fail_queries: false,
                search: FakeSearch::default(),
            }
        }

        /// An available handle whose search queries `responder` answers.
        pub fn test_with_responder(responder: EverythingResponder) -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results: HashMap::new(),
                fail_queries: false,
                search: FakeSearch::with(responder),
            }
        }

        /// Every search query run so far, oldest first.
        pub fn recorded_queries(&self) -> Vec<EverythingQuery> {
            self.search.recorded()
        }

        /// A handle whose service is installed but not answering yet.
        pub fn test_not_ready() -> Self {
            Self {
                availability: EverythingAvailability::NotReady,
                results: HashMap::new(),
                fail_queries: false,
                search: FakeSearch::default(),
            }
        }

        pub fn test_available_error() -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results: HashMap::new(),
                fail_queries: true,
                search: FakeSearch::default(),
            }
        }

        pub fn availability(&self) -> EverythingAvailability {
            self.availability.clone()
        }

        pub fn query_folder_size(&self, _path: &Path) -> Result<Option<u64>, EverythingError> {
            self.query_folder_sizes(&[_path.to_string_lossy().into_owned()], &|| false)
                .map(|sizes| sizes.values().next().copied().flatten())
        }

        pub fn query_folder_sizes(
            &self,
            paths: &[String],
            should_cancel: &dyn Fn() -> bool,
        ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
            if self.availability != EverythingAvailability::Available || self.fail_queries {
                return Err(EverythingError::NotReady);
            }

            if should_cancel() {
                return Ok(HashMap::new());
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

        pub fn run_query(
            &self,
            query: &EverythingQuery,
        ) -> Result<EverythingQueryResult, EverythingError> {
            if self.availability != EverythingAvailability::Available || self.fail_queries {
                return Err(EverythingError::NotReady);
            }
            self.search.run(query).ok_or(EverythingError::NotReady)
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
    use super::{EverythingQuery, EverythingQueryResult, EverythingResponder, FakeSearch};
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
        search: FakeSearch,
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
                search: FakeSearch::default(),
            }
        }

        /// An available handle whose search queries `responder` answers.
        pub fn test_with_responder(responder: EverythingResponder) -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results: HashMap::new(),
                fail_queries: false,
                search: FakeSearch::with(responder),
            }
        }

        /// Every search query run so far, oldest first.
        pub fn recorded_queries(&self) -> Vec<EverythingQuery> {
            self.search.recorded()
        }

        /// A handle whose service is installed but not answering yet.
        pub fn test_not_ready() -> Self {
            Self {
                availability: EverythingAvailability::NotReady,
                results: HashMap::new(),
                fail_queries: false,
                search: FakeSearch::default(),
            }
        }

        pub fn test_available_error() -> Self {
            Self {
                availability: EverythingAvailability::Available,
                results: HashMap::new(),
                fail_queries: true,
                search: FakeSearch::default(),
            }
        }

        pub fn availability(&self) -> EverythingAvailability {
            self.availability.clone()
        }

        pub fn query_folder_size(&self, _path: &Path) -> Result<Option<u64>, EverythingError> {
            self.query_folder_sizes(&[_path.to_string_lossy().into_owned()], &|| false)
                .map(|sizes| sizes.values().next().copied().flatten())
        }

        pub fn query_folder_sizes(
            &self,
            paths: &[String],
            should_cancel: &dyn Fn() -> bool,
        ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
            if self.availability != EverythingAvailability::Available || self.fail_queries {
                return Err(EverythingError::Unsupported);
            }

            if should_cancel() {
                return Ok(HashMap::new());
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

        pub fn run_query(
            &self,
            query: &EverythingQuery,
        ) -> Result<EverythingQueryResult, EverythingError> {
            if self.availability != EverythingAvailability::Available || self.fail_queries {
                return Err(EverythingError::Unsupported);
            }
            self.search.run(query).ok_or(EverythingError::Unsupported)
        }
    }
}

#[cfg(not(windows))]
pub use platform::*;
