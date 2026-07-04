use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jwalk::WalkDir;

#[derive(Debug)]
pub enum ManualSizeError {
    Cancelled,
    Timeout,
    Io(std::io::Error),
    Walk(jwalk::Error),
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

impl From<jwalk::Error> for ManualSizeError {
    fn from(value: jwalk::Error) -> Self {
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

    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(0);
    }

    let mut total = 0_u64;

    // `jwalk` walks the tree on a parallel worker pool that reads ahead of the
    // consuming loop below. If we only bailed out of the loop, those already
    // dispatched directory reads would keep churning through the whole subtree
    // after the caller has cancelled (e.g. the pane navigated away), pegging
    // the CPU for seconds. Pruning inside `process_read_dir` — which runs on
    // the worker threads for every directory read — clears the children of
    // each newly read directory once the job is cancelled, so the parallel
    // descent collapses at the source instead of running to completion.
    let prune_cancel = Arc::clone(cancel);
    let iterator = WalkDir::new(path)
        .follow_links(false)
        .sort(true)
        .process_read_dir(move |_depth, _path, _state, children| {
            if prune_cancel.load(Ordering::Relaxed) {
                children.clear();
            }
        })
        .try_into_iter()?;

    for entry in iterator {
        if cancel.load(Ordering::Relaxed) {
            return Err(ManualSizeError::Cancelled);
        }

        if started_at.elapsed() >= timeout {
            return Err(ManualSizeError::Timeout);
        }

        let entry = entry?;
        if entry.depth() == 0 {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => return Err(ManualSizeError::Walk(error)),
        };

        if metadata.file_type().is_symlink() || metadata.is_dir() {
            continue;
        }

        total = total.saturating_add(metadata.len());
    }

    // Cancel-pruning in `process_read_dir` can drain the iterator to nothing
    // the instant a job is cancelled, so the in-loop guard above may never run.
    // Re-check here so a pruned walk reports the cancellation instead of
    // returning a bogus partial size as success. (A fully drained walk that
    // merely reached its timeout on the final entry completed successfully, so
    // there is no post-loop timeout check — its total is correct.)
    if cancel.load(Ordering::Relaxed) {
        return Err(ManualSizeError::Cancelled);
    }

    Ok(total)
}
