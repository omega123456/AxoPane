use serde::{Deserialize, Serialize};

pub use crate::app_picker::types::{
    GetDefaultApplicationRequest, GetDefaultApplicationResponse, ListApplicationsResponse, MacApp,
    SetDefaultApplicationRequest,
};
pub use crate::directory_session::{
    BeginNavigationRequest, BeginNavigationResponse, GetSessionRangeRequest, ReleaseSessionRequest,
    ReleaseSessionResponse, ReviseSessionViewRequest, SessionBaseline, SessionRangePage,
    SessionRangeResponse, SessionRejection, ViewParams, SESSION_PAGE_SIZE,
};
pub use crate::fs::{
    DirectoryEntry, ListDirOptions, ListDirResponse, ListTreeChildrenOptions,
    ListTreeChildrenResponse, SortDirection, SortKey, TreeChildEntry, TreeExpandability,
};
pub use crate::item_counts::{
    ActiveItemsSortReady, ActiveItemsSortRequest, ActiveItemsSortResponse,
    ActiveItemsSortSuperseded, ItemCountEvent, ItemCountRequestContext, ItemCountResult,
    VisibleItemCountsRequest,
};
pub use crate::native_menu::types::{
    InvokeNativeMenuRequest, LoadNativeMenuRequest, LoadNativeMenuResponse,
    NativeMenuCanonicalActionKind, NativeMenuIcon, NativeMenuIconKind, NativeMenuItem,
    NativeMenuTargetKind,
};
pub use crate::ops::{
    ConflictInfo, ConflictResolution, OpItem, OpKind, OpProgress, OpSnapshot, OpStatus,
    StartOpRequest,
};
pub use crate::persist::{Config as AppConfig, Session as SessionState};
pub use crate::size::{EverythingStatus, SizeSource, SizeStateKind};
pub use crate::trash::TrashEntry;
pub use crate::volumes::VolumeInfo;
pub use crate::watch::patch::{MetadataDelta, RowDelta, SessionPatch};
pub use crate::watch::{DirPatch as WatchDirPatch, WatchTarget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneShell {
    pub id: String,
    pub title: String,
    pub path: String,
    pub placeholder_heading: String,
    pub placeholder_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRoot {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialShellResponse {
    pub panes: Vec<PaneShell>,
    pub tree_roots: Vec<TreeRoot>,
}

/// A single log line forwarded from the frontend logger (`app-log-commands.ts`).
///
/// `category` defaults to `frontend` and `details` carries the serialized log
/// context when present.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFrontendRequest {
    pub level: String,
    pub message: String,
    pub category: Option<String>,
    pub details: Option<String>,
}

pub type ListDirRequest = ListDirOptions;
pub type ListTreeChildrenRequest = ListTreeChildrenOptions;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigRequest {
    pub config: AppConfig,
}

/// Request to change the backend's minimum capture level.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLogLevelRequest {
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSessionRequest {
    pub session: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirPatchEvent {
    pub tab_id: String,
    pub path: String,
    pub reason: String,
    pub changed: Vec<DirEntryPatch>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryPatch {
    pub path: String,
    pub entry: Option<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeStateEvent {
    pub path: String,
    pub state: SizeStateKind,
    pub source: SizeSource,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSizeRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSizesRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestIconsRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconStateEvent {
    pub path: String,
    pub icon_data_url: Option<String>,
}

/// A listing-derived identity for a previewable file. Directory entries are
/// deliberately excluded: folders retain their ordinary large icon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailCandidateRequest {
    pub path: String,
    pub modified_unix_seconds: u64,
    pub size_bytes: u64,
    pub is_directory: bool,
    pub priority: ThumbnailPriority,
    pub order: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ThumbnailPriority {
    Visible,
    Ahead,
    Behind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestThumbnailsRequest {
    pub pane_id: String,
    pub tab_id: String,
    pub path: String,
    pub generation: u64,
    pub revision: u64,
    pub candidates: Vec<ThumbnailCandidateRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestThumbnailsResponse {
    pub revision: u64,
    pub accepted_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelThumbnailsRequest {
    pub pane_id: String,
    pub tab_id: String,
    pub path: String,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ThumbnailResultKind {
    Ready,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ThumbnailQuality {
    Low,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailResultEvent {
    pub pane_id: String,
    pub tab_id: String,
    pub path: String,
    pub generation: u64,
    pub fingerprint_path: String,
    pub modified_unix_seconds: u64,
    pub size_bytes: u64,
    pub state: ThumbnailResultKind,
    pub quality: Option<ThumbnailQuality>,
    pub data_url: Option<String>,
}

/// Batch of representative single-item (`File`/`Folder`) requests whose
/// derived cache keys should be background-warmed. Each element reproduces
/// exactly the request shape a single-row right-click would build, so the
/// warmed key equals the interactive key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmNativeMenusRequest {
    pub requests: Vec<LoadNativeMenuRequest>,
}

/// The cache keys newly inserted by a `warm_native_menus` call. Returned only
/// by the `test-utils` command variant for assertions; the production
/// (non-`test-utils`) command is fire-and-forget and returns nothing to the
/// frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmNativeMenusResponse {
    pub warmed_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelSizeRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelSizesRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEntryRequest {
    pub parent: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEntryRequest {
    pub path: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntriesRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTrashResponse {
    pub entries: Vec<TrashEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreTrashRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFromTrashRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPathRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileClipboardMode {
    Copy,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WriteFileClipboardRequest {
    pub mode: FileClipboardMode,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MenuActionStatus {
    pub handled: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EjectVolumeRequest {
    pub mount_root: String,
}

impl MenuActionStatus {
    pub fn handled_with_message(message: impl Into<String>) -> Self {
        Self {
            handled: true,
            message: Some(message.into()),
        }
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self {
            handled: false,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowPropertiesRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenWithRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelSizeResponse {
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelSizesResponse {
    pub cancelled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTabWatchRequest {
    pub target: Option<WatchTarget>,
    /// Optional already-published session entries for `target.path`. When
    /// supplied, these establish the watch baseline without a second scan.
    pub entries: Option<Vec<DirectoryEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumesChangedEvent {
    pub volumes: Vec<VolumeInfo>,
}

pub type QueueProgressEvent = OpProgress;
pub type QueueConflictEvent = ConflictInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderOpsRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpIdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveConflictRequest {
    pub id: String,
    pub resolution: ConflictResolution,
    pub apply_to_all: bool,
    pub rename_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchErrorEvent {
    pub path: String,
    pub message: String,
}
