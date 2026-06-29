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
