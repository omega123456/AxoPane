use std::fs;
use std::sync::Arc;

use file_explorer_lib::directory_session::{
    BeginNavigationRequest, BeginNavigationResponse, DirectorySessionService, ViewParams,
};
use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::volumes::registry::VolumeRegistry;
use serde_json::json;
use tauri::ipc::InvokeBody;
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tempfile::tempdir;

const BEGIN_DIRECTORY_SESSION: &str = "begin_directory_session";
const MOCK_WINDOW_LABEL: &str = "main";
const SUCCESS_CALLBACK: u32 = 0;
const ERROR_CALLBACK: u32 = 1;

fn invoke_url() -> tauri::Url {
    let origin = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    origin.parse().expect("invoke url")
}

fn navigation_request(path: &str) -> BeginNavigationRequest {
    BeginNavigationRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        path: path.to_string(),
        view: ViewParams {
            sort_key: SortKey::Name,
            sort_direction: SortDirection::Asc,
            filter: String::new(),
            show_hidden: false,
            include_item_counts: false,
        },
    }
}

#[test]
fn begin_directory_session_resolves_its_navigation_services_from_managed_state() {
    let directory = tempdir().expect("temp dir");
    fs::write(directory.path().join("first.txt"), b"first").expect("write first");
    fs::write(directory.path().join("second.txt"), b"second").expect("write second");

    let app = mock_builder()
        .manage(Arc::new(DirectorySessionService::default()))
        .manage(Arc::new(VolumeRegistry::default()))
        .manage(Arc::new(ResourceCoordinator::new()))
        .invoke_handler(tauri::generate_handler![commands::begin_directory_session])
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, MOCK_WINDOW_LABEL, Default::default())
        .build()
        .expect("webview");

    let payload = navigation_request(&directory.path().to_string_lossy());
    let response = get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: BEGIN_DIRECTORY_SESSION.into(),
            callback: tauri::ipc::CallbackFn(SUCCESS_CALLBACK),
            error: tauri::ipc::CallbackFn(ERROR_CALLBACK),
            url: invoke_url(),
            body: InvokeBody::Json(json!({ "payload": payload })),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .expect("begin directory session succeeds");
    let navigation: BeginNavigationResponse =
        response.deserialize().expect("begin navigation response");

    assert_eq!(navigation.total_rows, 2);
}
