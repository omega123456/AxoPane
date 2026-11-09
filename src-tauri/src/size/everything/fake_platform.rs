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
        if !cfg!(windows) {
            return Err(EverythingError::Unsupported);
        }
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

    pub fn test_with_responder(responder: EverythingResponder) -> Self {
        Self {
            availability: EverythingAvailability::Available,
            results: HashMap::new(),
            fail_queries: false,
            search: FakeSearch::with(responder),
        }
    }

    pub fn recorded_queries(&self) -> Vec<EverythingQuery> {
        self.search.recorded()
    }

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

    fn accepts_queries(&self) -> bool {
        let available = self.availability == EverythingAvailability::Available;
        available && !self.fail_queries
    }

    pub fn query_folder_sizes(
        &self,
        paths: &[String],
        should_cancel: &dyn Fn() -> bool,
    ) -> Result<HashMap<String, Option<u64>>, EverythingError> {
        if !self.accepts_queries() {
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
        if !self.accepts_queries() {
            return Err(EverythingError::Unsupported);
        }
        self.search.run(query).ok_or(EverythingError::Unsupported)
    }
}
