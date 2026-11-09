use std::collections::HashSet;

use super::matcher::EXACT;
use super::path_style::PathStyle;
use super::query::ParsedQuery;
use super::rank::{self, Scored};
use super::{Candidate, CandidateBatch};

pub const PAGE_SIZE: usize = 50;
pub const CANDIDATE_CAP: usize = 5_000;
pub const BROWSE_PARENT_CAP: usize = 128;
pub const LISTED_TIER: u8 = EXACT;

const POOL_PRUNE_THRESHOLD: usize = 2 * CANDIDATE_CAP;

pub struct TieredCandidate {
    pub tier: u8,
    pub candidate: Candidate,
}

#[derive(Default)]
pub struct CandidatePool {
    found: Vec<TieredCandidate>,
    truncated: bool,
}

impl CandidatePool {
    pub fn extend(&mut self, hits: impl IntoIterator<Item = TieredCandidate>) {
        self.found.extend(hits);
        if self.found.len() > POOL_PRUNE_THRESHOLD {
            self.keep_best();
        }
    }

    pub fn push_listed(&mut self, candidate: Candidate) -> bool {
        if self.is_full() {
            self.truncated = true;
            return false;
        }
        self.found.push(TieredCandidate {
            tier: LISTED_TIER,
            candidate,
        });
        true
    }

    pub fn is_over_cap(&self) -> bool {
        self.found.len() > CANDIDATE_CAP
    }

    pub fn is_full(&self) -> bool {
        self.found.len() >= CANDIDATE_CAP
    }

    pub fn mark_truncated(&mut self) {
        self.truncated = true;
    }

    pub fn into_batch(mut self) -> CandidateBatch {
        if self.found.len() > CANDIDATE_CAP {
            self.keep_best();
        }
        CandidateBatch {
            candidates: self.found.into_iter().map(|hit| hit.candidate).collect(),
            truncated: self.truncated,
        }
    }

    fn keep_best(&mut self) {
        self.found.sort_by_key(|hit| hit.tier);
        self.found.truncate(CANDIDATE_CAP);
        self.truncated = true;
    }
}

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

    pub fn page(&self, offset: usize) -> &[Scored] {
        let start = offset.min(self.hits.len());
        let end = start.saturating_add(PAGE_SIZE).min(self.hits.len());
        &self.hits[start..end]
    }
}
