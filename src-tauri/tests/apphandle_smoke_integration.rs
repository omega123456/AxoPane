use file_explorer_lib::ipc::commands;
use tauri::test::{mock_builder, mock_context, noop_assets};

#[test]
fn can_build_a_mock_app_for_command_wrappers() {
    let _app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("build app");

    let volumes = commands::list_volumes().expect("list volumes");
    assert!(!volumes.is_empty());
}
