use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Sum the byte size of a directory tree to drive byte-accurate progress.
///
/// `follow_symlinks` should match what the caller does with a link. Copy/move
/// preserve a symlink itself, while callers that deliberately traverse targets
/// can opt in to the cycle-guarded path below. IO errors are skipped so
/// measurement never aborts the operation.
pub fn measure_tree_size(path: &Path, cancel: &Arc<AtomicBool>, follow_symlinks: bool) -> u64 {
    if !follow_symlinks {
        return measure_tree_size_without_following_links(path, cancel);
    }
    // Seed the starting directory itself (like `copy_dir_recursive`'s `visited`
    // guard) so a symlink cycling straight back to `path` is caught on its
    // first hop rather than being traversed once before detection.
    let mut visited = match path.canonicalize() {
        Ok(canonical) => HashSet::from([canonical]),
        Err(_) => HashSet::new(),
    };
    measure_tree_size_inner(path, cancel, follow_symlinks, &mut visited)
}

fn measure_tree_size_without_following_links(path: &Path, cancel: &Arc<AtomicBool>) -> u64 {
    let Ok(entries) = crate::traversal::walk(
        path,
        crate::traversal::TraversalOptions::default(),
        Arc::clone(cancel),
    ) else {
        return 0;
    };
    entries
        .take_while(|entry| !matches!(entry, Err(crate::traversal::TraversalError::Cancelled)))
        .filter_map(Result::ok)
        .filter_map(|entry| walked_entry_size(&entry))
        .fold(0_u64, u64::saturating_add)
}

fn walked_entry_size(entry: &crate::traversal::TraversalEntry) -> Option<u64> {
    if entry.file_type.is_dir() {
        return None;
    }
    if entry.file_type.is_symlink() {
        return fs::symlink_metadata(&entry.path)
            .ok()
            .map(|metadata| metadata.len());
    }
    fs::metadata(&entry.path)
        .ok()
        .map(|metadata| metadata.len())
}

fn measure_tree_size_inner(
    path: &Path,
    cancel: &Arc<AtomicBool>,
    follow_symlinks: bool,
    visited: &mut HashSet<PathBuf>,
) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .take_while(|_| !cancel.load(Ordering::Relaxed))
        .map(|entry| measure_entry_size(&entry, cancel, follow_symlinks, visited))
        .fold(0_u64, u64::saturating_add)
}

fn measure_entry_size(
    entry: &fs::DirEntry,
    cancel: &Arc<AtomicBool>,
    follow_symlinks: bool,
    visited: &mut HashSet<PathBuf>,
) -> u64 {
    let Ok(file_type) = entry.file_type() else {
        return 0;
    };
    if file_type.is_symlink() && !follow_symlinks {
        return 0;
    }
    if file_type.is_symlink() {
        return measure_symlink_target_size(&entry.path(), cancel, follow_symlinks, visited);
    }
    if file_type.is_dir() {
        return measure_tree_size_inner(&entry.path(), cancel, follow_symlinks, visited);
    }
    if !file_type.is_file() {
        return 0;
    }
    entry.metadata().map_or(0, |metadata| metadata.len())
}

/// Resolve a symlink's target and measure it, recursing (with cycle
/// protection) if the target is a directory.
fn measure_symlink_target_size(
    link: &Path,
    cancel: &Arc<AtomicBool>,
    follow_symlinks: bool,
    visited: &mut HashSet<PathBuf>,
) -> u64 {
    let Ok(target_metadata) = fs::metadata(link) else {
        return 0;
    };
    if !target_metadata.is_dir() {
        return target_metadata.len();
    }
    let Ok(canonical) = link.canonicalize() else {
        return 0;
    };
    if !visited.insert(canonical.clone()) {
        return 0;
    }
    let total = measure_tree_size_inner(link, cancel, follow_symlinks, visited);
    visited.remove(&canonical);
    total
}

pub(super) fn measure_path_size(
    path: &Path,
    cancel: &Arc<AtomicBool>,
    follow_symlinks: bool,
) -> u64 {
    let Ok(link_metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if link_metadata.file_type().is_symlink() {
        if !follow_symlinks {
            return link_metadata.len();
        }
        return measure_symlink_target_size(path, cancel, follow_symlinks, &mut HashSet::new());
    }
    if link_metadata.is_dir() {
        return measure_tree_size(path, cancel, follow_symlinks);
    }
    link_metadata.len()
}
