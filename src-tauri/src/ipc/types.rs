use serde::{Deserialize, Serialize};

pub use crate::app_picker::types::{
    GetDefaultApplicationRequest, GetDefaultApplicationResponse, ListApplicationsResponse, MacApp,
    SetDefaultApplicationRequest,
};
pub use crate::fs::{
    DirectoryEntry, ListDirOptions, ListDirResponse, ListTreeChildrenOptions,
    ListTreeChildrenResponse, TreeChildEntry,
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
pub use crate::volumes::VolumeInfo;
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
pub struct CancelSizeRequest {
    pub path: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompressArchiveRequest {
    pub paths: Vec<String>,
    pub destination_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractArchiveRequest {
    pub paths: Vec<String>,
    pub destination_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelSizeResponse {
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTabWatchRequest {
    pub target: Option<WatchTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTabRequest {
    pub target: WatchTarget,
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
