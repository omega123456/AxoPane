use std::collections::HashMap;
use std::sync::Mutex;

use crate::fs::DirectoryEntry;

/// Number of entries carried inline in the `start_list_dir` response and per
/// streamed `list_chunk` event. Chosen so the frontend parses the directory
/// listing in small, non-blocking slices across animation frames instead of
/// deserializing an entire 20k+ entry array in one main-thread task.
pub const LIST_CHUNK_SIZE: usize = 500;

/// Tracks the most recent listing request per tab so a background streaming
/// task can bail as soon as a newer navigation supersedes it.
///
/// Each `start_list_dir` call bumps the tab's counter and takes the new value
/// as its request id; the streaming loop checks [`ListingService::is_current`]
/// before emitting each chunk and stops once the tab has moved on. Keeping this
/// per-tab (rather than global) means the two panes never cancel each other.
#[derive(Default)]
pub struct ListingService {
    counters: Mutex<HashMap<String, u64>>,
}

impl ListingService {
    /// Records and returns the next request id for `tab_id` (monotonic per tab,
    /// starting at 1). Supersedes any previous listing for the same tab.
    pub fn next_request_id(&self, tab_id: &str) -> u64 {
        let mut counters = self.counters.lock().expect("listing counters lock");
        let next = counters.get(tab_id).copied().unwrap_or(0) + 1;
        counters.insert(tab_id.to_string(), next);
        next
    }

    /// True while `request_id` is still the latest listing issued for `tab_id`.
    pub fn is_current(&self, tab_id: &str, request_id: u64) -> bool {
        self.counters
            .lock()
            .expect("listing counters lock")
            .get(tab_id)
            .copied()
            == Some(request_id)
    }
}

/// Splits an already-sorted listing into the inline first chunk (returned in
/// the `start_list_dir` response) and the remaining entries to be streamed as
/// `list_chunk` events. When the listing fits in a single chunk the rest is
/// empty and no streaming is needed.
pub fn split_first_chunk(
    mut entries: Vec<DirectoryEntry>,
    chunk_size: usize,
) -> (Vec<DirectoryEntry>, Vec<DirectoryEntry>) {
    if entries.len() <= chunk_size {
        return (entries, Vec::new());
    }

    let rest = entries.split_off(chunk_size);
    (entries, rest)
}
