#[test]
fn binary_entrypoint_runs_the_test_utils_app_stub() {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_file-explorer"))
        .status()
        .expect("spawn app binary");
    assert!(status.success());
}

#[test]
fn resolved_app_config_dir_maps_last_component_in_debug() {
    let base = std::path::Path::new("parent").join("com.axopane.app");
    let resolved = file_explorer_lib::resolved_app_config_dir(&base);

    #[cfg(debug_assertions)]
    {
        assert_eq!(
            resolved,
            std::path::Path::new("parent").join("com.axopane.app-dev")
        );
    }

    #[cfg(not(debug_assertions))]
    {
        assert_eq!(resolved, base);
    }
}

#[test]
fn webview_navigation_rejects_dropped_files() {
    let app = tauri::Url::parse("tauri://localhost").expect("app URL");
    let dev = tauri::Url::parse("http://localhost:1420").expect("dev URL");

    for file in ["file:///tmp/readme.txt", "file:///C:/Temp/readme.txt"] {
        assert!(!file_explorer_lib::allow_webview_navigation(
            &tauri::Url::parse(file).expect("file URL")
        ));
    }
    assert!(file_explorer_lib::allow_webview_navigation(&app));
    assert!(file_explorer_lib::allow_webview_navigation(&dev));
}
