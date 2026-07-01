#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;

use file_explorer_lib::file_icons::{icon_data_url_for_path, icon_pool, resolve_icon};
use tempfile::tempdir;

#[test]
fn native_icon_lookup_never_touches_the_shell_under_test_utils() {
    let fixture = tempdir().expect("temp dir");
    let exe_path = fixture.path().join("installer.exe");
    fs::write(&exe_path, "binary").expect("exe");

    assert_eq!(icon_data_url_for_path(&exe_path, false), None);
    assert_eq!(icon_data_url_for_path(&exe_path, true), None);
    assert_eq!(resolve_icon(&exe_path, false), None);
    assert_eq!(resolve_icon(&exe_path, true), None);
}

#[test]
fn resolve_icon_skips_directories_and_non_executable_extensions() {
    let fixture = tempdir().expect("temp dir");
    let dir_path = fixture.path().join("subdir");
    fs::create_dir(&dir_path).expect("subdir");
    let text_path = fixture.path().join("readme.txt");
    fs::write(&text_path, "text").expect("readme");

    assert_eq!(resolve_icon(&dir_path, true), None);
    assert_eq!(resolve_icon(&text_path, false), None);
    assert_eq!(resolve_icon(Path::new("no-extension"), false), None);
}

#[test]
fn icon_pool_is_available_and_reusable() {
    let pool = icon_pool().expect("icon pool should build");
    let second = icon_pool().expect("icon pool is memoized");
    assert!(std::ptr::eq(pool, second));

    let (sender, receiver) = std::sync::mpsc::channel();
    pool.spawn(move || {
        let _ = sender.send(());
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("icon pool executed the spawned job");

    assert_eq!(common::bootstrap_message(), "phase-1-common");
}
