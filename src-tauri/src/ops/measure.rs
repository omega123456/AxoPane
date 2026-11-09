use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn measure_tree_size(path: &Path, cancel: &Arc<AtomicBool>, follow_symlinks: bool) -> u64 {
    if !follow_symlinks {
        return measure_tree_size_without_following_links(path, cancel);
    }
    let mut visited = match path.canonicalize() {
        Ok(canonical) => HashSet::from([canonical]),
        Err(_) => HashSet::new(),
    };
    let mut measure = LinkFollowingMeasure {
        cancel,
        visited: &mut visited,
    };
    measure.tree_size(path)
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

struct LinkFollowingMeasure<'walk> {
    cancel: &'walk Arc<AtomicBool>,
    visited: &'walk mut HashSet<PathBuf>,
}

impl LinkFollowingMeasure<'_> {
    fn tree_size(&mut self, path: &Path) -> u64 {
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        let cancel = Arc::clone(self.cancel);
        entries
            .flatten()
            .take_while(|_| !cancel.load(Ordering::Relaxed))
            .map(|entry| self.entry_size(&entry))
            .fold(0_u64, u64::saturating_add)
    }

    fn entry_size(&mut self, entry: &fs::DirEntry) -> u64 {
        let Ok(file_type) = entry.file_type() else {
            return 0;
        };
        if file_type.is_symlink() {
            return self.symlink_target_size(&entry.path());
        }
        if file_type.is_dir() {
            return self.tree_size(&entry.path());
        }
        if !file_type.is_file() {
            return 0;
        }
        entry.metadata().map_or(0, |metadata| metadata.len())
    }

    fn symlink_target_size(&mut self, link: &Path) -> u64 {
        let Ok(target_metadata) = fs::metadata(link) else {
            return 0;
        };
        if !target_metadata.is_dir() {
            return target_metadata.len();
        }
        let Ok(canonical) = link.canonicalize() else {
            return 0;
        };
        if !self.visited.insert(canonical.clone()) {
            return 0;
        }
        let total = self.tree_size(link);
        self.visited.remove(&canonical);
        total
    }
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
        let mut visited = HashSet::new();
        let mut measure = LinkFollowingMeasure {
            cancel,
            visited: &mut visited,
        };
        return measure.symlink_target_size(path);
    }
    if link_metadata.is_dir() {
        return measure_tree_size(path, cancel, follow_symlinks);
    }
    link_metadata.len()
}
