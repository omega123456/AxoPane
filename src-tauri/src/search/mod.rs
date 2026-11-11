pub mod everything_plan;
pub mod everything_strategy;
pub mod file_scan;
pub mod matcher;
pub mod path_style;
pub mod prefilter;
pub mod query;
pub mod rank;
pub mod results;
pub mod service;
pub mod source;
pub mod tree_scan;

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: String,
    pub is_dir: bool,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateBatch {
    pub candidates: Vec<Candidate>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    NotReady,
    Unavailable,
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

pub struct SearchRun<'run, Source: ?Sized> {
    pub source: &'run Source,
    pub options: &'run SearchOptions,
    pub control: &'run Control<'run>,
}

pub struct Control<'a> {
    pub cancelled: &'a dyn Fn() -> bool,
    pub expired: &'a dyn Fn() -> bool,
}

impl Control<'_> {
    pub fn proceed(&self) -> Result<bool, SearchError> {
        if (self.cancelled)() {
            return Err(SearchError::Cancelled);
        }
        Ok(!(self.expired)())
    }
}
