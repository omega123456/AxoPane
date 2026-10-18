use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::traversal::TraversalError;

#[derive(Debug)]
pub enum ManualSizeError {
    Cancelled,
    Timeout,
    Io(std::io::Error),
    Walk(TraversalError),
}

impl std::fmt::Display for ManualSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "size request cancelled"),
            Self::Timeout => write!(f, "size request timed out"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Walk(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ManualSizeError {}

impl From<std::io::Error> for ManualSizeError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<TraversalError> for ManualSizeError {
    fn from(value: TraversalError) -> Self {
        Self::Walk(value)
    }
}

pub fn calculate(
    path: &Path,
    cancel: &Arc<AtomicBool>,
    timeout: Duration,
) -> Result<u64, ManualSizeError> {
    let started_at = Instant::now();
    let metadata = std::fs::symlink_metadata(path)?;

    // macOS sizes a symlink as its target, resolved once here (a dangling or
    // looping link is an I/O error); the walk itself never follows links.
    #[cfg(target_os = "macos")]
    if metadata.file_type().is_symlink() {
        let target = std::fs::canonicalize(path)?;
        return if target.is_dir() {
            subtree_total(&target, cancel, started_at, timeout)
        } else {
            Ok(0)
        };
    }

    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(0);
    }

    subtree_total(path, cancel, started_at, timeout)
}

/// macOS totals come from the parallel `getattrlistbulk` walker.
#[cfg(target_os = "macos")]
fn subtree_total(
    path: &Path,
    cancel: &Arc<AtomicBool>,
    started_at: Instant,
    timeout: Duration,
) -> Result<u64, ManualSizeError> {
    use super::fast_walk::{self, WalkOptions};

    fast_walk::subtree_total(
        path,
        &WalkOptions::new(Arc::clone(cancel), started_at.checked_add(timeout)),
    )
}

#[cfg(not(target_os = "macos"))]
fn subtree_total(
    path: &Path,
    cancel: &Arc<AtomicBool>,
    started_at: Instant,
    timeout: Duration,
) -> Result<u64, ManualSizeError> {
    use crate::traversal::{walk, TraversalOptions};
    use std::sync::atomic::Ordering;

    let mut total = 0_u64;

    let iterator = walk(path, TraversalOptions::default(), Arc::clone(cancel))?;

    for entry in iterator {
        if cancel.load(Ordering::Relaxed) {
            return Err(ManualSizeError::Cancelled);
        }

        if started_at.elapsed() >= timeout {
            return Err(ManualSizeError::Timeout);
        }

        let entry = entry?;
        let metadata = std::fs::symlink_metadata(&entry.path)?;

        if metadata.file_type().is_symlink() || metadata.is_dir() {
            continue;
        }

        total = total.saturating_add(metadata.len());
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(ManualSizeError::Cancelled);
    }

    Ok(total)
}
