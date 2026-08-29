use std::fs;

use std::path::Path;

use file_explorer_lib::launch::{
    elevation_failure_detail, launch_directory, launch_failure_detail, open_path, restart_as_admin,
    shell_execute_succeeded, should_prompt_open_with, OpenPathError, OPEN_WITH_VERB, RUNAS_VERB,
};
use tempfile::tempdir;

#[cfg(feature = "test-utils")]
#[test]
fn open_path_uses_the_safe_test_fallback_for_existing_paths() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("report.txt");
    fs::write(&path, b"report").expect("seed");

    let error = open_path(&path).expect_err("test-utils blocks real shell launching");
    assert!(matches!(error, OpenPathError::Unsupported));
    assert_eq!(
        error.to_string(),
        "opening paths is unsupported in this build"
    );
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn open_path_reports_io_errors_for_missing_paths() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing.txt");

    let error = open_path(&missing).expect_err("missing path");
    match error {
        OpenPathError::Io { path, source } => {
            assert!(path.ends_with("missing.txt"));
            assert!(!source.to_string().is_empty());
        }
        other => panic!("expected io error, got {other:?}"),
    }
}

#[test]
fn launch_directory_returns_the_paths_parent_folder() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("links.bat");

    assert_eq!(launch_directory(&path).as_deref(), Some(fixture.path()));
}

#[test]
fn launch_directory_ignores_bare_relative_file_names() {
    assert_eq!(launch_directory(Path::new("links.bat")), None);
}

#[test]
fn shell_execute_treats_only_codes_above_thirty_two_as_success() {
    assert!(!shell_execute_succeeded(0));
    assert!(!shell_execute_succeeded(31));
    assert!(!shell_execute_succeeded(32));
    assert!(shell_execute_succeeded(33));
    assert!(shell_execute_succeeded(42));
}

#[test]
fn missing_associations_fall_back_to_the_open_with_picker() {
    // SE_ERR_NOASSOC (31) is what `.iso` produced before the default verb fix,
    // and SE_ERR_ASSOCINCOMPLETE (27) is its half-registered sibling.
    assert!(should_prompt_open_with(31));
    assert!(should_prompt_open_with(27));
    assert_eq!(OPEN_WITH_VERB, "openas");
}

#[test]
fn other_launch_failures_do_not_prompt_the_open_with_picker() {
    for status in [0, 2, 8, 26, 28, 30, 32, 33] {
        assert!(
            !should_prompt_open_with(status),
            "status {status} must not open the picker"
        );
    }
}

#[test]
fn launch_failure_detail_reports_the_shell_status_code() {
    assert_eq!(
        launch_failure_detail(31),
        "ShellExecuteW returned status code 31"
    );
}

#[test]
fn a_refused_uac_prompt_reads_as_a_refusal_not_a_status_code() {
    // SE_ERR_ACCESSDENIED (5) is what a "runas" launch returns when the user
    // dismisses the UAC prompt.
    assert_eq!(
        elevation_failure_detail(5),
        "the administrator permission request was refused"
    );
    assert_eq!(RUNAS_VERB, "runas");
}

#[test]
fn other_elevation_failures_keep_the_shell_status_code() {
    assert_eq!(
        elevation_failure_detail(2),
        "ShellExecuteW returned status code 2"
    );
}

#[cfg(feature = "test-utils")]
#[test]
fn restart_as_admin_uses_the_safe_test_fallback() {
    let error = restart_as_admin().expect_err("test-utils blocks real elevation");
    assert!(matches!(error, OpenPathError::Unsupported));
}
