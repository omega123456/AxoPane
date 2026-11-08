use super::{
    build_exact_folder_or_queries, map_everything_result_sizes, EverythingQuery,
    EverythingQueryResult, EverythingRow, EverythingSort, EVERYTHING_BATCH_CHUNK_SIZE,
};
use std::collections::HashMap;
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

impl EverythingError {
    fn from_code(code: u32) -> Self {
        if code == EVERYTHING_ERROR_IPC {
            return Self::NotReady;
        }
        Self::QueryFailed(code)
    }
}

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

/// # Safety
/// `T` must match the exported symbol's real signature.
unsafe fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, EverythingError> {
    library
        .get::<T>(name)
        .map(|symbol| *symbol)
        .map_err(|_| EverythingError::DllUnavailable)
}

impl EverythingHandle {
    pub fn load() -> Result<Self, EverythingError> {
        let path = dll_candidates()
            .into_iter()
            .find(|candidate| candidate.exists())
            .unwrap_or_else(|| PathBuf::from("Everything64.dll"));

        let library = unsafe { Library::new(path) }.map_err(|_| EverythingError::DllUnavailable)?;
        let library = Arc::new(library);

        // SAFETY: each symbol is resolved by its documented Everything SDK
        // name and stored with that export's declared signature.
        unsafe {
            Ok(Self {
                query_lock: Mutex::new(()),
                set_search_w: symbol(&library, b"Everything_SetSearchW\0")?,
                set_request_flags: symbol(&library, b"Everything_SetRequestFlags\0")?,
                set_max: symbol(&library, b"Everything_SetMax\0")?,
                query_w: symbol(&library, b"Everything_QueryW\0")?,
                get_num_results: symbol(&library, b"Everything_GetNumResults\0")?,
                get_result_size: symbol(&library, b"Everything_GetResultSize\0")?,
                get_result_path_w: symbol(&library, b"Everything_GetResultPathW\0")?,
                get_result_file_name_w: symbol(&library, b"Everything_GetResultFileNameW\0")?,
                get_last_error: symbol(&library, b"Everything_GetLastError\0")?,
                is_db_loaded: symbol(&library, b"Everything_IsDBLoaded\0")?,
                set_match_path: symbol(&library, b"Everything_SetMatchPath\0")?,
                set_match_case: symbol(&library, b"Everything_SetMatchCase\0")?,
                set_match_whole_word: symbol(&library, b"Everything_SetMatchWholeWord\0")?,
                set_regex: symbol(&library, b"Everything_SetRegex\0")?,
                set_sort: symbol(&library, b"Everything_SetSort\0")?,
                set_offset: symbol(&library, b"Everything_SetOffset\0")?,
                get_tot_results: symbol(&library, b"Everything_GetTotResults\0")?,
                is_folder_result: symbol(&library, b"Everything_IsFolderResult\0")?,
                get_result_date_modified: symbol(&library, b"Everything_GetResultDateModified\0")?,
                _library: library,
            })
        }
    }

    pub fn availability(&self) -> EverythingAvailability {
        let database_loaded = unsafe { (self.is_db_loaded)() };
        if database_loaded {
            return EverythingAvailability::Available;
        }
        EverythingAvailability::NotReady
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

        let chunks = paths
            .chunks(EVERYTHING_BATCH_CHUNK_SIZE)
            .take_while(|_| !should_cancel());
        for chunk in chunks {
            sizes.extend(self.query_chunk_sizes(chunk)?);
        }

        Ok(sizes)
    }

    fn query_chunk_sizes(
        &self,
        chunk: &[String],
    ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
        let search = build_exact_folder_or_queries(chunk, EVERYTHING_BATCH_CHUNK_SIZE)
            .into_iter()
            .next()
            .expect("chunk query");
        let result = self.execute(
            &EverythingQuery {
                search,
                sort: EverythingSort::NameAscending,
                offset: 0,
                max: chunk.len() as u32,
            },
            EVERYTHING_REQUEST_FILE_NAME | EVERYTHING_REQUEST_PATH | EVERYTHING_REQUEST_SIZE,
        )?;
        let results: Vec<(String, String, Option<u64>)> = result
            .rows
            .into_iter()
            .map(|row| (row.path, row.file_name, row.size))
            .collect();
        Ok(map_everything_result_sizes(chunk, &results))
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
        let wide_search = crate::win32::wide_nul(&query.search);
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
                return Err(EverythingError::from_code((self.get_last_error)()));
            }

            let result_count = (self.get_num_results)();
            let rows: Vec<EverythingRow> = (0..result_count)
                .filter_map(|index| self.result_row(index, flags))
                .collect();
            Ok(EverythingQueryResult {
                rows,
                total: (self.get_tot_results)(),
            })
        }
    }

    /// Reads one row of the query just run under `query_lock`; rows
    /// without a path or name are skipped.
    fn result_row(&self, index: u32, flags: u32) -> Option<EverythingRow> {
        unsafe {
            let path = crate::win32::string_from_wide_ptr((self.get_result_path_w)(index))?;
            let file_name =
                crate::win32::string_from_wide_ptr((self.get_result_file_name_w)(index))?;
            let mut size = 0_i64;
            let size = (flags & EVERYTHING_REQUEST_SIZE != 0
                && (self.get_result_size)(index, &mut size as *mut i64)
                && size >= 0)
                .then_some(size as u64);
            let mut modified = 0_u64;
            let modified_filetime = (flags & EVERYTHING_REQUEST_DATE_MODIFIED != 0
                && (self.get_result_date_modified)(index, &mut modified as *mut u64) != 0)
                .then_some(modified);
            Some(EverythingRow {
                path,
                file_name,
                is_folder: (self.is_folder_result)(index) != 0,
                size,
                modified_filetime,
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
