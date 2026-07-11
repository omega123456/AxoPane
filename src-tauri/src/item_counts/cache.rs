use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::bounded_cache::BoundedCache;

pub const ITEM_COUNT_CACHE_LIMIT: usize = 50_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemCountKey {
    pub path: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ItemCountState {
    Unknown,
    Pending,
    Exact { value: u64 },
    Unavailable,
    Failed { message: String },
}

impl ItemCountState {
    pub fn value(&self) -> Option<u64> {
        match self {
            Self::Exact { value } => Some(*value),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ItemCountCache {
    entries: BoundedCache<ItemCountKey, ItemCountState>,
}

impl Default for ItemCountCache {
    fn default() -> Self {
        Self::new(ITEM_COUNT_CACHE_LIMIT)
    }
}

impl ItemCountCache {
    pub fn new(limit: usize) -> Self {
        Self {
            entries: BoundedCache::new(limit, limit),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn state(&mut self, path: &str, generation: u64) -> ItemCountState {
        self.entries
            .get(&key(path, generation))
            .cloned()
            .unwrap_or(ItemCountState::Unknown)
    }
    /// Returns true only for the caller that transitioned an unknown entry to pending.
    pub fn begin(&mut self, path: &str, generation: u64) -> bool {
        let key = key(path, generation);
        if matches!(self.entries.get(&key), Some(ItemCountState::Pending)) {
            return false;
        }
        if matches!(
            self.entries.get(&key),
            Some(ItemCountState::Exact { .. } | ItemCountState::Unavailable)
        ) {
            return false;
        }
        self.entries.insert(key, ItemCountState::Pending, 1);
        true
    }
    pub fn resolve(&mut self, path: &str, generation: u64, state: ItemCountState) {
        self.entries.insert(key(path, generation), state, 1);
    }
    /// A direct-child mutation changes only this directory's observable generation.
    pub fn invalidate_generation(&mut self, path: &str, generation: u64) {
        self.entries
            .retain(|entry, _| !(same_path(&entry.path, path) && entry.generation <= generation));
    }
}

fn key(path: &str, generation: u64) -> ItemCountKey {
    ItemCountKey {
        // Cache identity must survive equivalent display forms (notably
        // macOS's /var -> /private/var spelling and Windows canonical drive
        // forms), otherwise a viewport request and the session snapshot can
        // recount the same directory under two keys.
        path: crate::fs::canonicalize_dir(Path::new(path))
            .map(|canonical| crate::fs::display_path_from_path(&canonical))
            .unwrap_or_else(|_| crate::fs::display_path_from_path(Path::new(path))),
        generation,
    }
}
fn same_path(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    crate::fs::display_path_from_path(Path::new(left))
        == crate::fs::display_path_from_path(Path::new(right))
}
