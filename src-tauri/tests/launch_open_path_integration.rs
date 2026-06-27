use std::fs;

use file_explorer_lib::launch::{open_path, OpenPathError};
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
