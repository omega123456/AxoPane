use serde::{Deserialize, Serialize};

pub use crate::fs::{DirectoryEntry, ListDirOptions, ListDirResponse};
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

pub type ListDirRequest = ListDirOptions;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigRequest {
    pub config: AppConfig,
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
pub struct DeleteEntriesRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPathRequest {
    pub path: String,
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
