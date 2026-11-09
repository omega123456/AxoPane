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
const UNKNOWN_FILETIMES: [u64; 2] = [0, u64::MAX];
const EVERYTHING_SORT_NAME_ASCENDING: u32 = 1;
const EVERYTHING_SORT_DATE_MODIFIED_DESCENDING: u32 = 14;

/// Converts a FILETIME; `0` and `u64::MAX` mean "unknown".
pub fn filetime_to_system_time(filetime: u64) -> Option<SystemTime> {
    if UNKNOWN_FILETIMES.contains(&filetime) {
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
            normalized = normalized
                .trim_end_matches(crate::path_text::PATH_SEPARATORS)
                .to_string();
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
    let trimmed = path.trim_end_matches(crate::path_text::PATH_SEPARATORS);
    crate::path_text::is_drive_designator(trimmed)
        || (trimmed.starts_with(crate::path_text::UNC_PREFIX)
            && trimmed.matches('\\').count() <= UNC_SHARE_ROOT_MAX_SEPARATORS)
}

#[cfg(all(windows, not(feature = "test-utils")))]
#[path = "everything/windows_platform.rs"]
mod platform;

#[cfg(all(windows, not(feature = "test-utils")))]
pub use platform::*;

// Non-Windows targets never have Everything, and under `test-utils` Windows
// builds must never load the real `Everything64.dll` or hit the Everything IPC
// service. This stub mirrors the Windows API surface (including the unused
// `Available` / `NotReady` variants) so `size::mod` compiles identically, and
// always reports the safe unavailable fallback.
#[cfg(any(not(windows), feature = "test-utils"))]
#[path = "everything/fake_platform.rs"]
mod platform;

#[cfg(any(not(windows), feature = "test-utils"))]
pub use platform::*;
