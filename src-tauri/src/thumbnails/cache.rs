//! Bounded in-memory thumbnail cache. It deliberately has no visible-entry
//! protection; the renderer owns that presentation policy.

use super::types::{ThumbnailCacheKey, ThumbnailState};
use crate::bounded_cache::BoundedCache;

pub const MAX_CACHE_RECORDS: usize = 256;
pub const MAX_CACHE_WEIGHT: usize = 16 * 1024 * 1024;
pub const NEGATIVE_TTL_SECONDS: u64 = 5 * 60;

#[derive(Debug)]
struct CacheRecord {
    state: ThumbnailState,
    expires_at: Option<u64>,
}

#[derive(Debug)]
pub struct ThumbnailCache {
    records: BoundedCache<ThumbnailCacheKey, CacheRecord>,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self {
            records: BoundedCache::new(MAX_CACHE_RECORDS, MAX_CACHE_WEIGHT),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn get(&mut self, key: &ThumbnailCacheKey, now_seconds: u64) -> Option<ThumbnailState> {
        let record = self.records.get(key)?;
        if record
            .expires_at
            .is_some_and(|expiry| expiry <= now_seconds)
        {
            self.records.remove(key);
            return None;
        }
        Some(record.state.clone())
    }

    pub fn insert(&mut self, key: ThumbnailCacheKey, state: ThumbnailState, now_seconds: u64) {
        let state = match state {
            ThumbnailState::Ready { data_url } => {
                match super::types::validated_png_data_url(data_url) {
                    Ok(state) => state,
                    Err(_) => return,
                }
            }
            state => state,
        };
        let expires_at = state
            .is_negative()
            .then_some(now_seconds.saturating_add(NEGATIVE_TTL_SECONDS));
        let weight = state.weight();
        self.records
            .insert(key, CacheRecord { state, expires_at }, weight);
    }
}
