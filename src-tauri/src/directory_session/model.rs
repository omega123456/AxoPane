use serde::{Deserialize, Serialize};

use crate::fs::{DirectoryEntry, SortDirection, SortKey};

pub const SESSION_PAGE_SIZE: usize = 500;

pub type PaneId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NavigationRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WatchRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBaseline {
    pub session_id: SessionId,
    pub navigation_revision: NavigationRevision,
    pub watch_revision: WatchRevision,
    pub view_revision: ViewRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewParams {
    pub sort_key: SortKey,
    pub sort_direction: SortDirection,
    pub filter: String,
    pub show_hidden: bool,
    pub include_item_counts: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginNavigationRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub path: String,
    pub view: ViewParams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRangePage {
    pub page_index: u64,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginNavigationResponse {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub path: String,
    pub baseline: SessionBaseline,
    pub total_rows: u64,
    pub page_size: u64,
    pub first_page: SessionRangePage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionRangeRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub baseline: SessionBaseline,
    pub page_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRangeResponse {
    pub baseline: SessionBaseline,
    pub total_rows: u64,
    pub page: SessionRangePage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviseSessionViewRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub session_id: SessionId,
    pub view: ViewParams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSessionRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub session_id: SessionId,
    pub navigation_revision: NavigationRevision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSessionResponse {
    pub released: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SessionRejection {
    NoActiveSession,
    StaleSession,
    StaleNavigation,
    StaleWatch,
    StaleView,
    PageOutOfRange,
}

impl std::fmt::Display for SessionRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NoActiveSession => "no active directory session for this pane/tab",
            Self::StaleSession => "stale session id",
            Self::StaleNavigation => "stale navigation revision",
            Self::StaleWatch => "stale watch revision",
            Self::StaleView => "stale view revision",
            Self::PageOutOfRange => "page index out of range",
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for SessionRejection {}
