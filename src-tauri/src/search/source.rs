use super::SearchError;
use crate::size::everything::{
    EverythingAvailability, EverythingHandle, EverythingQuery, EverythingQueryResult,
};
use crate::size::index::IndexRead;

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
    fn read(&self, read: &mut dyn FnMut(IndexRead<'_>)) -> Result<(), SearchError>;

    fn file_names_available(&self) -> bool {
        let mut available = false;
        let readable = self.read(&mut |view| available = view.names.is_available());
        readable.is_ok() && available
    }
}

#[cfg(target_os = "macos")]
impl FolderTreeSource for crate::size::index::FolderSizeIndex {
    fn read(&self, read: &mut dyn FnMut(IndexRead<'_>)) -> Result<(), SearchError> {
        use crate::size::index::TreeUnavailable;
        self.read_index(read).map_err(|reason| match reason {
            TreeUnavailable::NotReady => SearchError::NotReady,
            TreeUnavailable::Unavailable => SearchError::Unavailable,
        })
    }
}

#[cfg(target_os = "macos")]
impl FolderTreeSource for crate::size::index::IndexHost {
    fn read(&self, read: &mut dyn FnMut(IndexRead<'_>)) -> Result<(), SearchError> {
        let index = self.index().ok_or(SearchError::Unavailable)?;
        FolderTreeSource::read(index.as_ref(), read)
    }
}
