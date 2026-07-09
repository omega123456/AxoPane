use std::collections::HashMap;
use std::sync::Mutex;

use crate::fs::{DirectoryEntry, SortDirection, SortKey};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingSession {
    pub tab_id: String,
    pub request_id: u64,
}

#[derive(Debug, Clone)]
pub struct CompletedListing {
    pub tab_id: String,
    pub request_id: u64,
    pub path: String,
    pub sort_key: SortKey,
    pub sort_direction: SortDirection,
    pub filter: String,
    pub show_hidden: bool,
    pub include_item_counts: bool,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Default)]
struct TabListingState {
    next_request_id: u64,
    active_request_id: Option<u64>,
    completed: Option<CompletedListing>,
}

#[derive(Default)]
pub struct ListingService {
    tabs: Mutex<HashMap<String, TabListingState>>,
}

impl ListingService {
    /// Starts a new listing session for `tab_id` (monotonic per tab, starting
    /// at 1) and supersedes any previously-active request for that same tab.
    pub fn begin_session(&self, tab_id: &str) -> ListingSession {
        let mut tabs = self.tabs.lock().expect("listing tabs lock");
        let state = tabs.entry(tab_id.to_string()).or_default();
        state.next_request_id += 1;
        state.active_request_id = Some(state.next_request_id);
        ListingSession {
            tab_id: tab_id.to_string(),
            request_id: state.next_request_id,
        }
    }

    /// True while `request_id` is still the latest listing issued for `tab_id`.
    pub fn is_current(&self, tab_id: &str, request_id: u64) -> bool {
        self.tabs
            .lock()
            .expect("listing tabs lock")
            .get(tab_id)
            .and_then(|state| state.active_request_id)
            == Some(request_id)
    }

    pub fn is_cancelled(&self, session: &ListingSession) -> bool {
        !self.is_current(&session.tab_id, session.request_id)
    }

    /// Stores the completed listing snapshot for the current session. Returns
    /// false when the session was superseded before completion, in which case
    /// the caller must treat the listing as ignored.
    pub fn complete_session(
        &self,
        session: &ListingSession,
        path: String,
        sort_key: SortKey,
        sort_direction: SortDirection,
        filter: String,
        show_hidden: bool,
        include_item_counts: bool,
        entries: Vec<DirectoryEntry>,
    ) -> bool {
        let mut tabs = self.tabs.lock().expect("listing tabs lock");
        let Some(state) = tabs.get_mut(&session.tab_id) else {
            return false;
        };

        if state.active_request_id != Some(session.request_id) {
            return false;
        }

        state.completed = Some(CompletedListing {
            tab_id: session.tab_id.clone(),
            request_id: session.request_id,
            path,
            sort_key,
            sort_direction,
            filter,
            show_hidden,
            include_item_counts,
            entries,
        });

        true
    }

    pub fn completed_for_tab(&self, tab_id: &str) -> Option<CompletedListing> {
        self.tabs
            .lock()
            .expect("listing tabs lock")
            .get(tab_id)
            .and_then(|state| state.completed.clone())
    }

    pub fn completed_seed_entries(
        &self,
        tab_id: &str,
        request_id: u64,
        target: &crate::watch::WatchTarget,
    ) -> Option<Vec<DirectoryEntry>> {
        let completed = self.completed_for_tab(tab_id)?;
        if completed.request_id != request_id {
            return None;
        }
        if completed.tab_id != tab_id {
            return None;
        }
        if !paths_match_for_context(&completed.path, &target.path)
            || completed.sort_key != target.sort_key
            || completed.sort_direction != target.sort_direction
            || completed.filter != target.filter
            || completed.show_hidden != target.show_hidden
            || completed.include_item_counts != target.include_item_counts
        {
            return None;
        }
        Some(completed.entries)
    }
}

pub fn paths_match_for_context(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }

    #[cfg(target_os = "windows")]
    {
        is_windows_path(left) && is_windows_path(right) && left.eq_ignore_ascii_case(right)
    }

    #[cfg(target_os = "macos")]
    {
        left.eq_ignore_ascii_case(right)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

#[cfg(target_os = "windows")]
fn is_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with(r"\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'))
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
