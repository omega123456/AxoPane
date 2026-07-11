use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ignore::{Walk, WalkBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraversalOptions {
    /// Include the root in output. Recursive consumers generally leave this
    /// false, because they already own root handling.
    pub include_root: bool,
}

impl Default for TraversalOptions {
    fn default() -> Self {
        Self {
            include_root: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraversalEntry {
    pub path: PathBuf,
    pub depth: usize,
    pub file_type: std::fs::FileType,
}

#[derive(Debug)]
pub enum TraversalError {
    Cancelled,
    Io(PathBuf, std::io::Error),
    DestinationEscape(PathBuf),
}

impl std::fmt::Display for TraversalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(formatter, "recursive traversal cancelled"),
            Self::Io(path, error) => write!(formatter, "{}: {error}", path.display()),
            Self::DestinationEscape(path) => {
                write!(formatter, "destination escape: {}", path.display())
            }
        }
    }
}

impl std::error::Error for TraversalError {}

/// Create the AxoPane recursive iterator. Ignore files and hidden-file policy
/// are disabled here because AxoPane, not a repository convention, controls
/// visibility. Directory links are never followed; they are returned as link
/// entries so copy/archive policies can preserve them deliberately.
pub fn walk(
    root: &Path,
    options: TraversalOptions,
    cancel: Arc<AtomicBool>,
) -> Result<TraversalIter, TraversalError> {
    let root_metadata = std::fs::symlink_metadata(root)
        .map_err(|error| TraversalError::Io(root.to_path_buf(), error))?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(TraversalError::Io(
            root.to_path_buf(),
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "traversal root is not a directory",
            ),
        ));
    }

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .follow_links(false)
        .threads(1);
    // Deliberately no `sort_by_file_name`: the `ignore` crate materializes
    // and sorts every directory's full child list up front to support it,
    // which is an unbounded buffer for a wide directory. Traversal order is
    // not a product contract (archive member sequence, size/count walks,
    // etc. are defined by member set/path/metadata/content, never order),
    // so this yields entries in whatever order the platform readdir call
    // returns them.
    Ok(TraversalIter {
        walk: builder.build(),
        root: root.to_path_buf(),
        include_root: options.include_root,
        cancel,
    })
}

pub struct TraversalIter {
    walk: Walk,
    root: PathBuf,
    include_root: bool,
    cancel: Arc<AtomicBool>,
}

impl Iterator for TraversalIter {
    type Item = Result<TraversalEntry, TraversalError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cancel.load(Ordering::Relaxed) {
            return Some(Err(TraversalError::Cancelled));
        }
        loop {
            let entry = self.walk.next()?;
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    return Some(Err(TraversalError::Io(
                        self.root.clone(),
                        std::io::Error::new(std::io::ErrorKind::Other, error.to_string()),
                    )))
                }
            };
            if !self.include_root && entry.path() == self.root {
                continue;
            }
            let file_type = match entry.file_type() {
                Some(file_type) => file_type,
                None => match std::fs::symlink_metadata(entry.path()) {
                    Ok(metadata) => metadata.file_type(),
                    Err(error) => {
                        return Some(Err(TraversalError::Io(entry.path().to_path_buf(), error)))
                    }
                },
            };
            return Some(Ok(TraversalEntry {
                path: entry.path().to_path_buf(),
                depth: entry.depth(),
                file_type,
            }));
        }
    }
}
