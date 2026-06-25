use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
    let iterator = WalkDir::new(path)
        .follow_links(false)
        .sort(true)
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

    Ok(total)
}
