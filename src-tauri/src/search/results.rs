//! The ranked hits for the latest query, served to the palette in pages.

use std::collections::HashSet;

use super::path_style::PathStyle;
use super::query::ParsedQuery;
use super::rank::{self, Scored};
use super::Candidate;

pub const PAGE_SIZE: usize = 50;
/// Candidates a backend may return for one query before it is truncated.
pub const CANDIDATE_CAP: usize = 5_000;
/// Folders whose children a `folder/` query lists.
pub const BROWSE_PARENT_CAP: usize = 128;

/// Scores and orders candidates, dropping duplicates and non-matches.
pub fn rank_candidates(
    candidates: impl IntoIterator<Item = Candidate>,
    query: &ParsedQuery,
    style: PathStyle,
) -> Vec<Scored> {
    let mut seen = HashSet::new();
    let mut hits: Vec<Scored> = candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.path.clone()))
        .filter_map(|candidate| rank::score(&candidate, query, style))
        .collect();
    hits.sort_by(rank::compare);
    hits
}

#[derive(Debug, Clone)]
pub struct ResultSet {
    query_id: u64,
    hits: Vec<Scored>,
    truncated: bool,
}

impl ResultSet {
    pub fn new(query_id: u64, hits: Vec<Scored>, truncated: bool) -> Self {
        Self {
            query_id,
            hits,
            truncated,
        }
    }

    pub fn query_id(&self) -> u64 {
        self.query_id
    }

    pub fn total(&self) -> usize {
        self.hits.len()
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Up to [`PAGE_SIZE`] hits starting at `offset`; empty past the end.
    pub fn page(&self, offset: usize) -> &[Scored] {
        let start = offset.min(self.hits.len());
        let end = start.saturating_add(PAGE_SIZE).min(self.hits.len());
        &self.hits[start..end]
    }
}
