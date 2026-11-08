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
    /// On Windows the fake loader succeeds but reports the service as
    /// unavailable, so no test path can ever observe a real Everything
    /// result; elsewhere Everything does not exist.
    pub fn load() -> Result<Self, EverythingError> {
        if cfg!(windows) {
            Ok(Self {
                availability: EverythingAvailability::Unavailable,
                results: HashMap::new(),
                fail_queries: false,
                search: FakeSearch::default(),
            })
        } else {
            Err(EverythingError::Unsupported)
        }
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
