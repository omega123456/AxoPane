//! Bounded in-memory thumbnail cache. It deliberately has no visible-entry
//! protection; the renderer owns that presentation policy.

use super::types::{ThumbnailCacheKey, ThumbnailState};
use crate::bounded_cache::BoundedCache;
use crate::ipc::types::ThumbnailQuality;

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
        let decision = self
            .records
            .get(&key)
            .map_or(InsertDecision::Replace, |existing| {
                insert_decision(existing, &state, now_seconds)
            });
        match decision {
            InsertDecision::KeepExisting => return false,
            InsertDecision::DeferRetry(record) => {
                let weight = record.state.weight();
                self.records.insert(key, record, weight);
                return false;
            }
            InsertDecision::Replace => {}
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

enum InsertDecision {
    Replace,
    KeepExisting,
    /// Keep the low-quality preview, but retry a better one only later.
    DeferRetry(CacheRecord),
}

fn insert_decision(
    existing: &CacheRecord,
    next: &ThumbnailState,
    now_seconds: u64,
) -> InsertDecision {
    let defer_retry = |retry_ttl: u64| {
        InsertDecision::DeferRetry(CacheRecord {
            state: existing.state.clone(),
            expires_at: existing.expires_at,
            retry_after: Some(now_seconds.saturating_add(retry_ttl)),
        })
    };
    match (&existing.state, next) {
        (
            ThumbnailState::Ready {
                quality: ThumbnailQuality::High,
                ..
            },
            _,
        ) => InsertDecision::KeepExisting,
        (
            ThumbnailState::Ready {
                quality: ThumbnailQuality::Low,
                ..
            },
            ThumbnailState::Unavailable,
        ) => defer_retry(NEGATIVE_TTL_SECONDS),
        (
            ThumbnailState::Ready {
                quality: ThumbnailQuality::Low,
                ..
            },
            ThumbnailState::Failed,
        ) => defer_retry(FAILED_TTL_SECONDS),
        (
            ThumbnailState::Ready {
                data_url: existing_url,
                quality: ThumbnailQuality::Low,
            },
            ThumbnailState::Ready {
                data_url,
                quality: ThumbnailQuality::Low,
            },
        ) if existing_url == data_url => InsertDecision::KeepExisting,
        _ => InsertDecision::Replace,
    }
}
