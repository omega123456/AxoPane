use crate::fs::{compare_entries, DirectoryEntry, SortOrder};

use super::model::ViewParams;

#[derive(Debug, Clone)]
pub struct SessionView {
    ordered: Vec<DirectoryEntry>,
}

impl SessionView {
    pub fn derive(snapshot: &[DirectoryEntry], params: &ViewParams) -> Self {
        let normalized_filter = params.filter.to_lowercase();
        let mut ordered: Vec<DirectoryEntry> = snapshot
            .iter()
            .filter(|entry| entry_matches_view(entry, params, &normalized_filter))
            .cloned()
            .collect();

        let order = SortOrder {
            key: params.sort_key,
            direction: params.sort_direction,
        };
        ordered.sort_by(|left, right| compare_entries(left, right, order));

        Self { ordered }
    }

    pub fn total_rows(&self) -> u64 {
        self.ordered.len() as u64
    }

    pub fn rows(&self) -> &[DirectoryEntry] {
        &self.ordered
    }

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
    entry.is_visible_with(params.show_hidden) && entry.name_matches_filter(normalized_filter)
}
