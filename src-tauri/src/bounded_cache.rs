//! Small deterministic LRU storage shared by metadata services.
//!
//! Domain modules own their key validity and invalidation policy; this type
//! only enforces entry and optional weight limits.

use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug)]
struct Record<V> {
    value: V,
    weight: usize,
    touched: u64,
}

pub struct Weighted<V> {
    pub value: V,
    pub weight: usize,
}

#[derive(Debug)]
pub struct BoundedCache<K, V> {
    entries: HashMap<K, Record<V>>,
    max_entries: usize,
    max_weight: usize,
    total_weight: usize,
    clock: u64,
}

impl<K: Eq + Hash + Clone, V> BoundedCache<K, V> {
    pub fn new(max_entries: usize, max_weight: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries,
            max_weight,
            total_weight: 0,
            clock: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.clock = self.clock.wrapping_add(1);
        let record = self.entries.get_mut(key)?;
        record.touched = self.clock;
        Some(&record.value)
    }

    pub fn insert(&mut self, key: K, entry: Weighted<V>) {
        let Weighted { value, weight } = entry;
        self.clock = self.clock.wrapping_add(1);
        if let Some(previous) = self.entries.remove(&key) {
            self.total_weight = self.total_weight.saturating_sub(previous.weight);
        }
        self.total_weight = self.total_weight.saturating_add(weight);
        self.entries.insert(
            key,
            Record {
                value,
                weight,
                touched: self.clock,
            },
        );
        self.evict();
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let record = self.entries.remove(key)?;
        self.total_weight = self.total_weight.saturating_sub(record.weight);
        Some(record.value)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.entries.retain(|key, record| keep(key, &record.value));
        self.total_weight = self
            .entries
            .values()
            .map(|record| record.weight)
            .fold(0, usize::saturating_add);
    }

    fn evict(&mut self) {
        while let Some(key) = self.eviction_candidate() {
            let _ = self.remove(&key);
        }
    }

    fn eviction_candidate(&self) -> Option<K> {
        let over_capacity =
            self.entries.len() > self.max_entries || self.total_weight > self.max_weight;
        if !over_capacity {
            return None;
        }
        self.entries
            .iter()
            .min_by_key(|(_, record)| record.touched)
            .map(|(key, _)| key.clone())
    }
}
