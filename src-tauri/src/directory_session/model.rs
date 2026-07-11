//! Core types for the directory-session contract: session/watch/view
//! revisions, the seekable range request/response shapes, and the lifecycle
//! records `directory_session::mod` keeps per pane.

use serde::{Deserialize, Serialize};

use crate::fs::{DirectoryEntry, SortDirection, SortKey};

/// Row page size for every range response. Matches the plan's approved
/// default (Functional Requirement 1 / Constraints & Assumptions).
pub const SESSION_PAGE_SIZE: usize = 500;

/// Identifies a pane. AxoPane has exactly two panes (`"left"` / `"right"`),
/// mirroring `watch::pane_scope`, but this stays a plain owned `String` so
/// callers do not need to import a pane-id enum from another module.
pub type PaneId = String;

/// A session id is opaque to callers: it is only ever compared for equality
/// against the value returned by `begin_navigation`. Monotonically increasing
/// per pane so a superseded session can never be mistaken for the current one
/// even if identifiers were reused across process restarts (they are not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub u64);

/// Bumped every time `begin_navigation` establishes a new session for a pane.
/// Distinct from [`SessionId`] so a caller can distinguish "this is a stale
/// session" from "this is a stale navigation attempt for the *current*
/// session" (e.g. a superseded in-flight `begin_navigation` call for the same
/// pane). Every accepted range/patch response carries the navigation
/// revision active when its session was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NavigationRevision(pub u64);

/// Bumped whenever the native watch backing a session is (re)established.
/// Watch mutations and range responses reference this so a stale watch
/// generation (e.g. after a resnapshot restart) cannot be silently accepted
/// as current.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WatchRevision(pub u64);

/// Bumped whenever the derived sort/filter *view* over the session's
/// unfiltered snapshot changes (new sort key/direction, new filter text, or a
/// watch mutation that reorders/adds/removes rows). Range requests are scoped
/// to one view revision; a stale view revision must be rejected rather than
/// served, per Functional Requirement 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewRevision(pub u64);

/// The complete revision set a session/watch/view baseline publishes and
/// every later request/response must match to be accepted. Carried by every
/// v2 range response so the frontend (Phase 4) can validate what it has
/// against what it is being asked to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBaseline {
    pub session_id: SessionId,
    pub navigation_revision: NavigationRevision,
    pub watch_revision: WatchRevision,
    pub view_revision: ViewRevision,
}

/// View parameters a session's derived projection is built from. Changing
/// sort/filter/show-hidden revises the view over the *same* unfiltered
/// snapshot (Decisions and constraints: "A session owns one filesystem
/// snapshot, not separate snapshots per filter/sort combination").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewParams {
    pub sort_key: SortKey,
    pub sort_direction: SortDirection,
    pub filter: String,
    pub show_hidden: bool,
    pub include_item_counts: bool,
}

/// Request to establish a new session for `pane_id`/`tab_id` at `path`,
/// retiring any prior session/watch/count/range ownership for that pane
/// first. Idempotent: calling this twice with the same `navigation_revision`
/// intent simply supersedes the previous attempt (the caller always gets a
/// freshly-issued, strictly increasing `NavigationRevision` back).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginNavigationRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub path: String,
    pub view: ViewParams,
}

/// One page of session rows. `entries.len()` is `<= SESSION_PAGE_SIZE`; the
/// final page of a view is short rather than padded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRangePage {
    pub page_index: u64,
    pub entries: Vec<DirectoryEntry>,
}

/// Response to `begin_navigation`: the established baseline, the resolved
/// (canonicalized) path, total row count for the current view, and the first
/// viewport page so the frontend does not need a second round trip just to
/// paint the top of the pane.
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

/// Idempotent page-index range request, scoped to one baseline. Any valid
/// `page_index` may be retried (forward or backward) and returns the same
/// content for as long as `baseline` remains current.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionRangeRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub baseline: SessionBaseline,
    pub page_index: u64,
}

/// Successful range response: total row count is echoed alongside the page so
/// the frontend never has to guess whether the view total has changed
/// between two range fetches (it also always carries `baseline` again to
/// remove any ambiguity about what was actually served).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRangeResponse {
    pub baseline: SessionBaseline,
    pub total_rows: u64,
    pub page: SessionRangePage,
}

/// Request to revise the sort/filter/show-hidden/item-count view of an
/// already-active session, without re-enumerating the filesystem. Scoped by
/// `pane_id`/`tab_id`/`session_id` (not the full baseline) because a view
/// revision is exactly what this request is about to advance — requiring the
/// caller to already know the *next* view revision would be circular.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviseSessionViewRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub session_id: SessionId,
    pub view: ViewParams,
}

/// Idempotent release request. Pane/tab/session/navigation revisions must all
/// be supplied so a release for an already-superseded session is a safe
/// no-op rather than accidentally tearing down a newer session that reused
/// the same pane/tab (e.g. rapid navigation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSessionRequest {
    pub pane_id: PaneId,
    pub tab_id: String,
    pub session_id: SessionId,
    pub navigation_revision: NavigationRevision,
}

/// Whether a `release_session` call actually retired an active session
/// (`true`) or found nothing to do because it was already retired/never
/// existed (`false`, and still a success — release is intentionally
/// idempotent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSessionResponse {
    pub released: bool,
}

/// Every reason a v2 request can be rejected rather than served. Kept
/// separate from filesystem I/O errors (`String` in this phase, matching the
/// existing command error convention) so callers can distinguish "stale
/// request, safe to ignore" from "real failure, should surface to the user".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SessionRejection {
    /// No active session exists for this pane/tab at all.
    NoActiveSession,
    /// A session exists but its id no longer matches (a newer
    /// `begin_navigation` superseded it).
    StaleSession,
    /// The session matches but the navigation revision is stale.
    StaleNavigation,
    /// The session/navigation match but the watch revision is stale (the
    /// watch was re-established, e.g. after an overflow resnapshot).
    StaleWatch,
    /// The session/navigation/watch match but the view revision is stale
    /// (sort/filter changed since the caller's baseline was issued).
    StaleView,
    /// `page_index` is out of range for the current view's total row count.
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
