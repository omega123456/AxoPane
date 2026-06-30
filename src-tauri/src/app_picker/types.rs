use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MacApp {
    pub name: String,
    pub bundle_path: String,
    pub bundle_id: Option<String>,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListApplicationsResponse {
    pub apps: Vec<MacApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetDefaultApplicationRequest {
    pub path: String,
    pub bundle_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetDefaultApplicationRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetDefaultApplicationResponse {
    pub app: Option<MacApp>,
}
