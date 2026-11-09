use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::size::index::exclusions::{ExclusionVerdict, PathVerdict};
use crate::size::IndexUnsupported;

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
pub use crate::search::service::{
    SearchBackendKind, SearchCapabilities, SearchHit, SearchPage, SearchPageRequest,
    SearchQueryRequest, SearchRejection,
};
pub use crate::search::SearchScope;
pub use crate::size::{FolderSizeIndexStatus, NaReason, RequestMode, SizeSource, SizeStateKind};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub na_reason: Option<NaReason>,
    #[serde(default)]
    pub is_lower_bound: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_roots: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_roots: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_more: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddIndexExclusionRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoveIndexExclusionRequest {
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IndexExclusionOutcomeKind {
    Added,
    Duplicate,
    InsideExisting,
    MergedChildren,
    AlwaysSkipped,
    RootRejected,
    HomeWarning,
    NotFound,
    NotAFolder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexExclusionOutcome {
    pub path: String,
    pub outcome: IndexExclusionOutcomeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_count: Option<usize>,
}

impl From<PathVerdict> for IndexExclusionOutcome {
    fn from(outcome: PathVerdict) -> Self {
        Self {
            path: path_string(&outcome.path),
            outcome: outcome_kind(&outcome.verdict),
            detail: outcome_detail(&outcome.verdict),
            merged_count: merged_count(&outcome.verdict),
        }
    }
}

fn outcome_detail(verdict: &ExclusionVerdict) -> Option<String> {
    match verdict {
        ExclusionVerdict::Duplicate(existing) | ExclusionVerdict::InsideExisting(existing) => {
            Some(path_string(existing))
        }
        _ => None,
    }
}

fn merged_count(verdict: &ExclusionVerdict) -> Option<usize> {
    match verdict {
        ExclusionVerdict::MergedChildren(children) => Some(children.len()),
        _ => None,
    }
}

fn outcome_kind(verdict: &ExclusionVerdict) -> IndexExclusionOutcomeKind {
    match verdict {
        ExclusionVerdict::Added => IndexExclusionOutcomeKind::Added,
        ExclusionVerdict::HomeWarning => IndexExclusionOutcomeKind::HomeWarning,
        ExclusionVerdict::MergedChildren(_) => IndexExclusionOutcomeKind::MergedChildren,
        ExclusionVerdict::Duplicate(_) => IndexExclusionOutcomeKind::Duplicate,
        ExclusionVerdict::InsideExisting(_) => IndexExclusionOutcomeKind::InsideExisting,
        ExclusionVerdict::AlwaysSkipped => IndexExclusionOutcomeKind::AlwaysSkipped,
        ExclusionVerdict::RootRejected => IndexExclusionOutcomeKind::RootRejected,
        ExclusionVerdict::NotFound => IndexExclusionOutcomeKind::NotFound,
        ExclusionVerdict::NotAFolder => IndexExclusionOutcomeKind::NotAFolder,
    }
}

pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn path_strings(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|path| path_string(path)).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddIndexExclusionResponse {
    pub exclusions: Vec<String>,
    pub outcomes: Vec<IndexExclusionOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexExclusionsResponse {
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PickIndexExclusionsResponse {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IndexCommandError {
    Unsupported,
}

impl From<IndexUnsupported> for IndexCommandError {
    fn from(_: IndexUnsupported) -> Self {
        Self::Unsupported
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSizeRequest {
    pub path: String,
    #[serde(default)]
    pub mode: Option<RequestMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSizesRequest {
    pub paths: Vec<String>,
    #[serde(default)]
    pub mode: Option<RequestMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeSubscriptionsRequest {
    pub pane_id: String,
    #[serde(default)]
    pub current_path: Option<String>,
    pub folder_paths: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmNativeMenusRequest {
    pub requests: Vec<LoadNativeMenuRequest>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetWindowThemeRequest {
    pub theme: Option<String>,
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
pub struct FileClipboardItem {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileClipboardResponse {
    pub mode: FileClipboardMode,
    pub items: Vec<FileClipboardItem>,
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
