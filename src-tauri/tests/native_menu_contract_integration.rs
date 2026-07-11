#[path = "common/mod.rs"]
mod common;

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    InvokeNativeMenuRequest, LoadNativeMenuRequest, NativeMenuIconKind, NativeMenuTargetKind,
    OpenWithRequest, ShowPropertiesRequest,
};
use file_explorer_lib::native_menu::NativeMenuService;

fn as_state<'a, T: Send + Sync + 'static>(value: &'a T) -> tauri::State<'a, T> {
    unsafe { std::mem::transmute::<&'a T, tauri::State<'a, T>>(value) }
}

#[test]
fn command_contracts_return_safe_test_utils_behavior() {
    let service = NativeMenuService::default();
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let response = commands::load_native_menu(
        LoadNativeMenuRequest {
            request_id: "req-contract".to_string(),
            target_kind: NativeMenuTargetKind::File,
            target_path: Some("C:\\fixture\\report.txt".to_string()),
            folder_path: Some("C:\\fixture".to_string()),
            selected_paths: vec!["C:\\fixture\\report.txt".to_string()],
        },
        as_state(&service),
    );

    assert_eq!(response.request_id, "req-contract");
    assert_eq!(response.items.len(), 3);
    assert_eq!(response.items[0].id, "fixture-target-file");
    let terminal_icon = response.items[2].icon.as_ref().expect("fixture icon");
    assert_eq!(terminal_icon.kind, NativeMenuIconKind::DataUrl);
    assert_eq!(terminal_icon.data_url, "data:image/png;base64,RkFLRQ==");
    assert_eq!(terminal_icon.alt.as_deref(), Some("Fixture icon"));

    let invoke_token = response.items[2]
        .invoke_token
        .clone()
        .expect("fake provider token");
    let invoke_result = commands::invoke_native_menu_action(
        InvokeNativeMenuRequest {
            token: invoke_token,
        },
        as_state(&service),
    );
    assert!(invoke_result.handled);
    assert_eq!(
        invoke_result.message.as_deref(),
        Some("invoked:fake.openTerminal")
    );

    let stale = commands::invoke_native_menu_action(
        InvokeNativeMenuRequest {
            token: "native:req-contract:9999".to_string(),
        },
        as_state(&service),
    );
    assert!(!stale.handled);
    assert_eq!(stale.message.as_deref(), Some("stale-or-unknown-token"));

    let properties = commands::show_properties(
        ShowPropertiesRequest {
            paths: vec!["C:\\fixture\\report.txt".to_string()],
        },
        as_state(&service),
    );
    assert!(!properties.handled);
    assert_eq!(properties.message.as_deref(), Some("unsupported"));

    let open_with = commands::open_with(
        OpenWithRequest {
            path: "C:\\fixture\\report.txt".to_string(),
        },
        as_state(&service),
    );
    assert!(!open_with.handled);
    assert_eq!(open_with.message.as_deref(), Some("unsupported"));
}
