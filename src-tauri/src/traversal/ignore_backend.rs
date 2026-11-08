use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ignore::{Walk, WalkBuilder};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TraversalOptions {
    /// Include the root in output. Recursive consumers generally leave this
    /// false, because they already own root handling.
    pub include_root: bool,
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
    if !super::is_real_directory(&root_metadata) {
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
        let include_root = self.include_root;
        let root = &self.root;
        let entry = self
            .walk
            .by_ref()
            .find(|entry| include_root || !is_root_entry(entry, root))?;
        let entry = entry.map_err(|error| {
            TraversalError::Io(root.clone(), std::io::Error::other(error.to_string()))
        });
        Some(entry.and_then(|entry| traversal_entry(&entry)))
    }
}

fn is_root_entry(entry: &Result<ignore::DirEntry, ignore::Error>, root: &Path) -> bool {
    matches!(entry, Ok(entry) if entry.path() == root)
}

fn traversal_entry(entry: &ignore::DirEntry) -> Result<TraversalEntry, TraversalError> {
    let file_type = match entry.file_type() {
        Some(file_type) => file_type,
        None => symlink_file_type(entry.path())?,
    };
    Ok(TraversalEntry {
        path: entry.path().to_path_buf(),
        depth: entry.depth(),
        file_type,
    })
}

fn symlink_file_type(path: &Path) -> Result<std::fs::FileType, TraversalError> {
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type())
        .map_err(|error| TraversalError::Io(path.to_path_buf(), error))
}
