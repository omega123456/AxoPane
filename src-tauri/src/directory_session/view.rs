//! Rust-owned view derivation: turns a session's immutable unfiltered
//! snapshot plus [`ViewParams`] into an ordered projection, without ever
//! re-enumerating the filesystem. Filtering and sorting are *view* changes
//! over the same snapshot (Phase 3 decision), never new snapshots.

use crate::fs::{compare_entries, DirectoryEntry};

use super::model::ViewParams;

/// The ordered, filtered projection of a session's snapshot for one set of
/// [`ViewParams`]. Holds owned clones of the matching entries in their final
/// display order — cheap relative to re-enumerating, and exactly what
/// `directory_session::paging` slices into pages.
#[derive(Debug, Clone)]
pub struct SessionView {
    ordered: Vec<DirectoryEntry>,
}

impl SessionView {
    /// Builds a view by filtering `snapshot` per `params` and sorting the
    /// result with the same ordering rules `fs::list_dir` uses, so v2 session
    /// listings and the legacy v1 chunk listings never visibly disagree on
    /// ordering for equivalent inputs.
    pub fn derive(snapshot: &[DirectoryEntry], params: &ViewParams) -> Self {
        let normalized_filter = params.filter.to_lowercase();
        let mut ordered: Vec<DirectoryEntry> = snapshot
            .iter()
            .filter(|entry| entry_matches_view(entry, params, &normalized_filter))
            .cloned()
            .collect();

        ordered.sort_by(|left, right| {
            compare_entries(left, right, params.sort_key, params.sort_direction)
        });

        Self { ordered }
    }

    pub fn total_rows(&self) -> u64 {
        self.ordered.len() as u64
    }

    /// The complete ordered/filtered row list. Used by watch-mutation
    /// reconciliation (Phase 5) to diff the previous and next views and
    /// classify the smallest applicable [`crate::watch::patch::SessionPatch`].
    /// Not the hot paging path — [`SessionView::page`] stays the primary
    /// accessor for ordinary range requests.
    pub fn rows(&self) -> &[DirectoryEntry] {
        &self.ordered
    }

    /// Returns the `[start, end)` slice of ordered entries for `page_index`
    /// at `page_size`, or `None` if `page_index` has no rows (out of range).
    /// An empty-but-valid final page (exactly at the boundary) still returns
    /// `Some(&[])` only when `page_index == 0` and the view itself is empty;
    /// any other out-of-range page index is `None` so callers can distinguish
    /// "valid empty view" from "asked for a page past the end".
    pub fn page(&self, page_index: u64, page_size: usize) -> Option<&[DirectoryEntry]> {
        if self.ordered.is_empty() {
            return (page_index == 0).then_some(&self.ordered[..]);
        }

        let start = (page_index as usize).checked_mul(page_size)?;
        if start >= self.ordered.len() {
            return None;
        }
        let end = (start + page_size).min(self.ordered.len());
        Some(&self.ordered[start..end])
    }
}

fn entry_matches_view(
    entry: &DirectoryEntry,
    params: &ViewParams,
    normalized_filter: &str,
) -> bool {
    if !params.show_hidden && (entry.is_hidden || entry.is_system) {
        return false;
    }

    normalized_filter.is_empty() || entry.name.to_lowercase().contains(normalized_filter)
}
