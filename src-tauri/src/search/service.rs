//! The search service behind the IPC commands: picks the backend, runs the
//! latest query (a new one cancels the previous), keeps its ranked hits and
//! serves them in pages.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::path_style::PathStyle;
use super::query::parse;
use super::rank::Scored;
use super::results::{rank_candidates, ResultSet};
use super::source::{EverythingSource, FolderTreeSource};
use super::{everything_strategy, tree_scan, Control, SearchError, SearchOptions, SearchScope};
use crate::ipc::executor::Cancellation;
use crate::size::SizeService;

/// Collection stops after this and returns what it found, marked truncated.
pub const SEARCH_BUDGET: Duration = Duration::from_millis(1_500);
/// The IPC deadline for one search request; longer than the budget so a
/// budget-limited search still answers.
pub const SEARCH_DEADLINE: Duration = Duration::from_secs(3);

pub enum SearchBackend {
    Everything(Arc<dyn EverythingSource>),
    Tree(Arc<dyn FolderTreeSource>),
    None,
}

impl SearchBackend {
    /// Shares the size service's backends: its Everything handle when one is
    /// loaded, else the macOS folder-size index.
    pub fn from_sizes(sizes: &SizeService) -> Self {
        if let Some(handle) = sizes.everything_handle() {
            return Self::Everything(handle);
        }
        #[cfg(target_os = "macos")]
        if let Some(index) = sizes.folder_size_index() {
            return Self::Tree(index);
        }
        Self::None
    }

    fn kind(&self) -> SearchBackendKind {
        match self {
            Self::Everything(_) => SearchBackendKind::Everything,
            Self::Tree(_) => SearchBackendKind::MacIndex,
            Self::None => SearchBackendKind::None,
        }
    }

    fn path_style(&self) -> PathStyle {
        match self {
            Self::Everything(_) => PathStyle::Windows,
            Self::Tree(_) | Self::None => PathStyle::Posix,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchBackendKind {
    Everything,
    MacIndex,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchCapabilities {
    pub backend: SearchBackendKind,
    /// The backend knows folder names only (the macOS index).
    pub folders_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchQueryRequest {
    pub query: String,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub scope: SearchScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchPageRequest {
    pub query_id: u64,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// The backend's path; every action uses it.
    pub path: String,
    /// NFC display name and parent; the ranges are UTF-16 offsets into them.
    pub name: String,
    pub parent_path: String,
    pub is_dir: bool,
    pub modified_at: Option<String>,
    pub name_ranges: Vec<[u32; 2]>,
    pub parent_ranges: Vec<[u32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub query_id: u64,
    pub offset: usize,
    pub total: usize,
    /// The backend had more matches than were ranked.
    pub truncated: bool,
    pub items: Vec<SearchHit>,
    pub capabilities: SearchCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SearchRejection {
    /// A newer query superseded this one, or its results were dropped.
    Stale,
    /// Fewer than three characters (and not an anchored browse).
    TooShort,
    NotReady,
    Unavailable,
    Failed {
        message: String,
    },
}

impl SearchRejection {
    /// Maps an IPC executor failure: cancellation means superseded.
    pub fn from_executor(message: String) -> Self {
        if message.contains("cancelled") {
            Self::Stale
        } else {
            Self::Failed { message }
        }
    }
}

impl From<SearchError> for SearchRejection {
    fn from(error: SearchError) -> Self {
        match error {
            SearchError::NotReady => Self::NotReady,
            SearchError::Unavailable => Self::Unavailable,
            SearchError::Cancelled => Self::Stale,
            SearchError::Failed(message) => Self::Failed { message },
        }
    }
}

/// Issued by [`SearchService::begin`] for one query.
pub struct SearchTicket {
    pub generation: u64,
    pub cancellation: Cancellation,
}

#[derive(Default)]
struct State {
    generation: u64,
    cancel: Cancellation,
    results: Option<Arc<ResultSet>>,
}

pub struct SearchService {
    backend: SearchBackend,
    home: Option<String>,
    budget: Duration,
    state: Mutex<State>,
}

impl SearchService {
    pub fn new(backend: SearchBackend, home: Option<String>) -> Self {
        Self::with_budget(backend, home, SEARCH_BUDGET)
    }

    pub fn with_budget(backend: SearchBackend, home: Option<String>, budget: Duration) -> Self {
        Self {
            backend,
            home,
            budget,
            state: Mutex::default(),
        }
    }

    pub fn capabilities(&self) -> SearchCapabilities {
        SearchCapabilities {
            backend: self.backend.kind(),
            folders_only: matches!(self.backend, SearchBackend::Tree(_)),
        }
    }

    /// Starts a query: cancels the running one and drops its results. Call
    /// before enqueueing, so a superseded query never starts its work.
    pub fn begin(&self) -> SearchTicket {
        let mut state = self.state();
        state.generation += 1;
        state.cancel.cancel();
        state.cancel = Cancellation::default();
        state.results = None;
        SearchTicket {
            generation: state.generation,
            cancellation: state.cancel.clone(),
        }
    }

    pub fn run(
        &self,
        ticket: SearchTicket,
        request: &SearchQueryRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<SearchPage, SearchRejection> {
        let style = self.backend.path_style();
        let query = parse(&request.query, style, self.home.as_deref())
            .map_err(|_| SearchRejection::TooShort)?;
        let options = SearchOptions {
            include_hidden: request.include_hidden,
            scope: request.scope,
        };
        let started = Instant::now();
        let is_cancelled = || cancelled() || ticket.cancellation.is_cancelled();
        let expired = || started.elapsed() >= self.budget;
        let control = Control {
            cancelled: &is_cancelled,
            expired: &expired,
        };
        let batch = match &self.backend {
            SearchBackend::Everything(source) => {
                everything_strategy::candidates(source.as_ref(), &query, &options, &control)
            }
            SearchBackend::Tree(source) => {
                tree_scan::candidates(source.as_ref(), &query, &options, &control)
            }
            SearchBackend::None => Err(SearchError::Unavailable),
        }?;
        let hits: Vec<Scored> = rank_candidates(batch.candidates, &query, style)
            .into_iter()
            .filter(|hit| match options.scope {
                SearchScope::All => true,
                SearchScope::Folders => hit.is_dir,
                SearchScope::Files => !hit.is_dir,
            })
            .collect();
        let results = Arc::new(ResultSet::new(ticket.generation, hits, batch.truncated));
        {
            let mut state = self.state();
            if state.generation != ticket.generation {
                return Err(SearchRejection::Stale);
            }
            state.results = Some(Arc::clone(&results));
        }
        Ok(self.page_of(&results, 0))
    }

    pub fn page(&self, request: &SearchPageRequest) -> Result<SearchPage, SearchRejection> {
        let results = self
            .state()
            .results
            .clone()
            .filter(|results| results.query_id() == request.query_id)
            .ok_or(SearchRejection::Stale)?;
        Ok(self.page_of(&results, request.offset))
    }

    /// The palette closed: stop the running query and free its results.
    pub fn close(&self) {
        let mut state = self.state();
        state.generation += 1;
        state.cancel.cancel();
        state.results = None;
    }

    fn page_of(&self, results: &ResultSet, offset: usize) -> SearchPage {
        SearchPage {
            query_id: results.query_id(),
            offset,
            total: results.total(),
            truncated: results.truncated(),
            items: results.page(offset).iter().map(to_hit).collect(),
            capabilities: self.capabilities(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("search state lock")
    }
}

/// The index stores no modified times, so a page's hits are stat-ed when
/// served (at most one page at a time).
fn to_hit(hit: &Scored) -> SearchHit {
    let modified = hit.modified.or_else(|| {
        std::fs::symlink_metadata(&hit.path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
    });
    SearchHit {
        path: hit.path.clone(),
        name: hit.name.clone(),
        parent_path: hit.parent.clone(),
        is_dir: hit.is_dir,
        modified_at: crate::fs::system_time_to_rfc3339(modified),
        name_ranges: hit
            .name_ranges
            .iter()
            .map(|&(start, end)| [start, end])
            .collect(),
        parent_ranges: hit
            .parent_ranges
            .iter()
            .map(|&(start, end)| [start, end])
            .collect(),
    }
}
