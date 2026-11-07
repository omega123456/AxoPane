//! The only backend calls search makes. Everything above these traits is
//! shared code; tests implement them directly.

use std::path::PathBuf;

use super::SearchError;
use crate::size::everything::{
    EverythingAvailability, EverythingHandle, EverythingQuery, EverythingQueryResult,
};
use crate::size::index::tree::FolderSizeTree;

pub trait EverythingSource: Send + Sync {
    fn run(&self, query: &EverythingQuery) -> Result<EverythingQueryResult, SearchError>;
}

impl EverythingSource for EverythingHandle {
    fn run(&self, query: &EverythingQuery) -> Result<EverythingQueryResult, SearchError> {
        if self.availability() != EverythingAvailability::Available {
            return Err(SearchError::NotReady);
        }
        self.run_query(query)
            .map_err(|error| SearchError::Failed(format!("{error:?}")))
    }
}

pub trait FolderTreeSource: Send + Sync {
    /// Runs `read` once under the tree's lock, with the protected roots
    /// whose contents must stay hidden. Keep each call short.
    fn read(&self, read: &mut dyn FnMut(&FolderSizeTree, &[PathBuf])) -> Result<(), SearchError>;
}

#[cfg(target_os = "macos")]
impl FolderTreeSource for crate::size::index::FolderSizeIndex {
    fn read(&self, read: &mut dyn FnMut(&FolderSizeTree, &[PathBuf])) -> Result<(), SearchError> {
        use crate::size::index::TreeUnavailable;
        self.read_tree(|tree, blocked| read(tree, blocked))
            .map_err(|reason| match reason {
                TreeUnavailable::NotReady => SearchError::NotReady,
                TreeUnavailable::Unavailable => SearchError::Unavailable,
            })
    }
}
