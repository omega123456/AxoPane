//! Bounded in-memory thumbnail cache. It deliberately has no visible-entry
//! protection; the renderer owns that presentation policy.

use super::types::{ThumbnailCacheKey, ThumbnailState};
use crate::bounded_cache::BoundedCache;

pub const MAX_CACHE_RECORDS: usize = 256;
pub const MAX_CACHE_WEIGHT: usize = 16 * 1024 * 1024;
pub const NEGATIVE_TTL_SECONDS: u64 = 5 * 60;
pub const FAILED_TTL_SECONDS: u64 = 30;

#[derive(Clone, Debug)]
struct CacheRecord {
    state: ThumbnailState,
    expires_at: Option<u64>,
    retry_after: Option<u64>,
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

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&mut self, key: &ThumbnailCacheKey, now_seconds: u64) -> Option<ThumbnailState> {
        self.get_with_upgrade(key, now_seconds)
            .map(|(state, _)| state)
    }

    pub fn get_with_upgrade(
        &mut self,
        key: &ThumbnailCacheKey,
        now_seconds: u64,
    ) -> Option<(ThumbnailState, bool)> {
        let record = self.records.get(key)?;
        if record
            .expires_at
            .is_some_and(|expiry| expiry <= now_seconds)
        {
            self.records.remove(key);
            return None;
        }
        let should_upgrade = matches!(
            record.state,
            ThumbnailState::Ready {
                quality: crate::ipc::types::ThumbnailQuality::Low,
                ..
            }
        ) && record.retry_after.is_none_or(|retry| retry <= now_seconds);
        Some((record.state.clone(), should_upgrade))
    }

    pub fn insert(
        &mut self,
        key: ThumbnailCacheKey,
        state: ThumbnailState,
        now_seconds: u64,
    ) -> bool {
        if let Some(existing) = self.records.get(&key).cloned() {
            let preserve_existing = match (&existing.state, &state) {
                (
                    ThumbnailState::Ready {
                        quality: crate::ipc::types::ThumbnailQuality::High,
                        ..
                    },
                    _,
                ) => true,
                (
                    ThumbnailState::Ready {
                        quality: crate::ipc::types::ThumbnailQuality::Low,
                        ..
                    },
                    next @ (ThumbnailState::Unavailable | ThumbnailState::Failed),
                ) => {
                    let retry_ttl = if matches!(next, ThumbnailState::Unavailable) {
                        NEGATIVE_TTL_SECONDS
                    } else {
                        FAILED_TTL_SECONDS
                    };
                    self.records.insert(
                        key,
                        CacheRecord {
                            state: existing.state.clone(),
                            expires_at: existing.expires_at,
                            retry_after: Some(now_seconds.saturating_add(retry_ttl)),
                        },
                        existing.state.weight(),
                    );
                    return false;
                }
                (
                    ThumbnailState::Ready {
                        data_url: existing_url,
                        quality: crate::ipc::types::ThumbnailQuality::Low,
                    },
                    ThumbnailState::Ready {
                        data_url,
                        quality: crate::ipc::types::ThumbnailQuality::Low,
                    },
                ) => existing_url == data_url,
                _ => false,
            };
            if preserve_existing {
                return false;
            }
        }
        let expires_at = match &state {
            ThumbnailState::Unavailable => Some(now_seconds.saturating_add(NEGATIVE_TTL_SECONDS)),
            ThumbnailState::Failed => Some(now_seconds.saturating_add(FAILED_TTL_SECONDS)),
            ThumbnailState::Ready { .. } => None,
        };
        let weight = state.weight();
        self.records.insert(
            key,
            CacheRecord {
                state,
                expires_at,
                retry_after: None,
            },
            weight,
        );
        true
    }
}
