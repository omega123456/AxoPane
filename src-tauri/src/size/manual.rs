use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::traversal::{is_real_directory, TraversalError};

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

#[derive(Clone, Copy)]
struct StopConditions<'stop> {
    cancel: &'stop Arc<AtomicBool>,
    started_at: Instant,
    timeout: Duration,
}

pub fn calculate(
    path: &Path,
    cancel: &Arc<AtomicBool>,
    timeout: Duration,
) -> Result<u64, ManualSizeError> {
    let stop = StopConditions {
        cancel,
        started_at: Instant::now(),
        timeout,
    };
    let metadata = std::fs::symlink_metadata(path)?;

    #[cfg(target_os = "macos")]
    if metadata.file_type().is_symlink() {
        let target = std::fs::canonicalize(path)?;
        if !target.is_dir() {
            return Ok(0);
        }
        return subtree_total(&target, &stop);
    }

    if !is_real_directory(&metadata) {
        return Ok(0);
    }

    subtree_total(path, &stop)
}

#[cfg(target_os = "macos")]
fn subtree_total(path: &Path, stop: &StopConditions<'_>) -> Result<u64, ManualSizeError> {
    let StopConditions {
        cancel,
        started_at,
        timeout,
    } = *stop;
    use super::fast_walk::{self, WalkOptions};

    fast_walk::subtree_total(
        path,
        &WalkOptions::new(Arc::clone(cancel), started_at.checked_add(timeout)),
    )
}

#[cfg(not(target_os = "macos"))]
fn subtree_total(path: &Path, stop: &StopConditions<'_>) -> Result<u64, ManualSizeError> {
    let StopConditions {
        cancel,
        started_at,
        timeout,
    } = *stop;
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

        let is_plain_file = !metadata.file_type().is_symlink() && !metadata.is_dir();
        if !is_plain_file {
            continue;
        }

        total = total.saturating_add(metadata.len());
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(ManualSizeError::Cancelled);
    }

    Ok(total)
}
