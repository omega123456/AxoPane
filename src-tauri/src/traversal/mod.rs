//! Bounded, serial recursive traversal shared by filesystem operations.
//!
//! This module deliberately uses `ignore`'s serial walker. Parallelism belongs
//! to `ResourceCoordinator`, so a single admitted job never creates a second
//! unbounded worker pool while walking a wide directory.

mod ignore_backend;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub use ignore_backend::{walk, TraversalEntry, TraversalError, TraversalOptions};

/// Lexically validate a relative archive member and ensure it cannot escape
/// `destination`. Existing ancestor symlinks are rejected as well: joining an
/// otherwise harmless relative path through one could write outside the root.
pub fn safe_destination(destination: &Path, member: &Path) -> Result<PathBuf, TraversalError> {
    if member.is_absolute()
        || member
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(TraversalError::DestinationEscape(member.to_path_buf()));
    }

    let candidate = destination.join(member);
    let mut ancestor = destination.to_path_buf();
    for component in member.components() {
        ancestor.push(component.as_os_str());
        if ancestor.exists() {
            let metadata = std::fs::symlink_metadata(&ancestor)
                .map_err(|error| TraversalError::Io(ancestor.clone(), error))?;
            if metadata.file_type().is_symlink() {
                return Err(TraversalError::DestinationEscape(candidate));
            }
        }
    }
    Ok(candidate)
}

/// Convenience for callers that only need cancellation-aware serial walking.
pub fn walk_without_links(
    root: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<impl Iterator<Item = Result<TraversalEntry, TraversalError>>, TraversalError> {
    walk(root, TraversalOptions::default(), Arc::clone(cancel))
}
