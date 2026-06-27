use file_explorer_lib::ipc::commands;

#[test]
fn can_build_a_real_app_handle_for_command_wrappers() {
    let app = tauri::Builder::default()
        .build(tauri::generate_context!())
        .expect("build app");

    let volumes = commands::list_volumes(app.handle().clone()).expect("list volumes");
    assert!(!volumes.is_empty());
}
