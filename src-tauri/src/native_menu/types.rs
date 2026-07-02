use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NativeMenuTargetKind {
    File,
    Folder,
    Multi,
    Mixed,
    DriveRoot,
    Background,
    Tree,
    Tab,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoadNativeMenuRequest {
    pub request_id: String,
    pub target_kind: NativeMenuTargetKind,
    pub target_path: Option<String>,
    pub folder_path: Option<String>,
    pub selected_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NativeMenuCanonicalActionKind {
    Open,
    OpenWith,
    Copy,
    CopyAsPath,
    Cut,
    Paste,
    Rename,
    Delete,
    Properties,
    Share,
    Compress,
    Extract,
    Refresh,
    NewFolder,
    NewFile,
    SelectAll,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NativeMenuIconKind {
    DataUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeMenuIcon {
    pub kind: NativeMenuIconKind,
    pub data_url: String,
    pub alt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeMenuItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub danger: bool,
    pub canonical_action_kind: Option<NativeMenuCanonicalActionKind>,
    pub normalized_verb: Option<String>,
    pub invoke_token: Option<String>,
    pub icon: Option<NativeMenuIcon>,
    pub children: Vec<NativeMenuItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoadNativeMenuResponse {
    pub request_id: String,
    pub items: Vec<NativeMenuItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InvokeNativeMenuRequest {
    pub token: String,
}
