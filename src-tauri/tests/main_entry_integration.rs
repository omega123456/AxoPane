#[test]
fn binary_entrypoint_runs_the_test_utils_app_stub() {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_file-explorer"))
        .status()
        .expect("spawn app binary");
    assert!(status.success());
}
