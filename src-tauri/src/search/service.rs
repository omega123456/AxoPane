use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::path_style::PathStyle;
use super::query::parse;
use super::rank::Scored;
use super::results::{rank_candidates, ResultSet};
use super::source::{EverythingSource, FolderTreeSource};
use super::{
    everything_strategy, tree_scan, Control, SearchError, SearchOptions, SearchRun, SearchScope,
};
use crate::ipc::executor::Cancellation;
use crate::size::SizeService;

pub const SEARCH_BUDGET: Duration = Duration::from_millis(1_500);
pub const SEARCH_DEADLINE: Duration = Duration::from_secs(3);

pub enum SearchBackend {
    Everything(Arc<dyn EverythingSource>),
    Tree(Arc<dyn FolderTreeSource>),
    None,
}

impl SearchBackend {
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
    pub path: String,
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
    pub truncated: bool,
    pub items: Vec<SearchHit>,
    pub capabilities: SearchCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SearchRejection {
    Stale,
    TooShort,
    NotReady,
    Unavailable,
    Failed { message: String },
}

impl SearchRejection {
    pub fn from_executor(message: String) -> Self {
        if message.contains("cancelled") {
            return Self::Stale;
        }
        Self::Failed { message }
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

pub struct SearchTicket {
    pub generation: u64,
    pub cancellation: Cancellation,
}

pub struct RunTicket<'run> {
    pub ticket: SearchTicket,
    pub cancelled: &'run dyn Fn() -> bool,
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
        run_ticket: RunTicket<'_>,
        request: &SearchQueryRequest,
    ) -> Result<SearchPage, SearchRejection> {
        let RunTicket { ticket, cancelled } = run_ticket;
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
                let run = SearchRun {
                    source: source.as_ref(),
                    options: &options,
                    control: &control,
                };
                everything_strategy::candidates(&run, &query)
            }
            SearchBackend::Tree(source) => {
                let run = SearchRun {
                    source: source.as_ref(),
                    options: &options,
                    control: &control,
                };
                tree_scan::candidates(&run, &query)
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
        self.publish_results(ticket.generation, &results)?;
        Ok(self.page_of(&results, 0))
    }

    fn publish_results(
        &self,
        generation: u64,
        results: &Arc<ResultSet>,
    ) -> Result<(), SearchRejection> {
        let mut state = self.state();
        if state.generation != generation {
            return Err(SearchRejection::Stale);
        }
        state.results = Some(Arc::clone(results));
        Ok(())
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
