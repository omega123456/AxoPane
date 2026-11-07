//! Search everywhere: one query grammar and one ranking for every backend.
//! Backends (Everything on Windows, the folder-size index on macOS) only
//! supply raw candidates; parsing, matching, ranking and paging live here
//! and are host-independent.

pub mod everything_plan;
pub mod everything_strategy;
pub mod matcher;
pub mod path_style;
pub mod query;
pub mod rank;
pub mod results;
pub mod service;
pub mod source;
pub mod tree_scan;

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// A file or folder a backend reported, before matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: String,
    pub is_dir: bool,
    pub modified: Option<SystemTime>,
}

/// What a backend found for one query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateBatch {
    pub candidates: Vec<Candidate>,
    /// More matches existed than were collected.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    /// The backend exists but cannot answer yet (Everything not running,
    /// index still booting).
    NotReady,
    /// No backend can search on this machine.
    Unavailable,
    /// A newer query superseded this one.
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchScope {
    #[default]
    All,
    Folders,
    Files,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub include_hidden: bool,
    pub scope: SearchScope,
}

/// Lets long-running candidate collection stop early.
pub struct Control<'a> {
    pub cancelled: &'a dyn Fn() -> bool,
    /// The time budget is spent: stop, but keep what was found.
    pub expired: &'a dyn Fn() -> bool,
}

impl Control<'_> {
    /// `Ok(true)` to continue, `Ok(false)` once the budget is spent, and
    /// `Err(Cancelled)` when superseded.
    pub fn proceed(&self) -> Result<bool, SearchError> {
        if (self.cancelled)() {
            return Err(SearchError::Cancelled);
        }
        Ok(!(self.expired)())
    }
}
