#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use file_explorer_lib::ops::{
    fallback_root, is_nested_copy_target, measure_tree_size, normalized_components, parent_dir,
    remove_source, remove_target, requested_pop, split_name, unique_name, volume_root_for,
};
use file_explorer_lib::volumes::VolumeInfo;
use tempfile::tempdir;

fn volume(mount_root: &str) -> VolumeInfo {
    VolumeInfo {
        mount_root: mount_root.to_string(),
        label: mount_root.to_string(),
        total_bytes: 1,
        free_bytes: 1,
        is_network: false,
        is_removable: false,
    }
}

#[test]
fn path_helpers_cover_name_root_and_component_logic() {
    let mut requested = vec!["first".to_string(), "second".to_string()];
    assert_eq!(requested_pop(&mut requested), Some("first".to_string()));
    assert_eq!(requested_pop(&mut requested), Some("second".to_string()));
    assert_eq!(requested_pop(&mut requested), None);

    assert_eq!(
        split_name("photo.png"),
        ("photo".to_string(), "png".to_string())
    );
    assert_eq!(split_name(".env"), (".env".to_string(), String::new()));
    assert_eq!(
        split_name("Makefile"),
        ("Makefile".to_string(), String::new())
    );

    if cfg!(windows) {
        assert_eq!(
            parent_dir("C:\\Users\\Omega\\report.txt"),
            "C:\\Users\\Omega"
        );
        assert_eq!(fallback_root("c:\\users\\omega"), "c:");
    } else {
        assert_eq!(parent_dir("/home/omega/report.txt"), "/home/omega");
        assert_eq!(fallback_root("/srv/share/docs"), "/srv");
        assert_eq!(fallback_root("/"), "/");
    }
    assert_eq!(parent_dir("report.txt"), String::new());

    let normalized = normalized_components(std::path::Path::new("alpha/./beta/../gamma"));
    assert_eq!(normalized.last().expect("last component"), "gamma");
    let parent_heavy = normalized_components(std::path::Path::new("../alpha/../../gamma"));
    assert!(parent_heavy.contains(&"..".to_string()));
    assert!(is_nested_copy_target(
        std::path::Path::new("alpha/beta"),
        std::path::Path::new("alpha/beta/nested"),
    ));
    assert!(!is_nested_copy_target(
        std::path::Path::new("alpha/beta"),
        std::path::Path::new("alpha/other"),
    ));

    let volumes = if cfg!(windows) {
        vec![volume("c:\\"), volume("c:\\users")]
    } else {
        vec![volume("/"), volume("/srv")]
    };
    let matched = if cfg!(windows) {
        volume_root_for(std::path::Path::new("C:\\Users\\Omega\\file.txt"), &volumes)
    } else {
        volume_root_for(std::path::Path::new("/srv/share/file.txt"), &volumes)
    };
    assert!(matched.ends_with("users") || matched == "/srv");

    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn filesystem_helpers_cover_measurement_cleanup_and_auto_names() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let nested = root.join("nested");
    fs::create_dir(&nested).expect("nested");
    fs::write(root.join("alpha.bin"), vec![0_u8; 4]).expect("alpha");
    fs::write(nested.join("beta.bin"), vec![0_u8; 6]).expect("beta");

    let cancel = Arc::new(AtomicBool::new(false));
    assert_eq!(measure_tree_size(root, &cancel), 10);

    fs::write(root.join("report.txt"), b"old").expect("report");
    fs::write(root.join("report (1).txt"), b"older").expect("report copy");
    assert_eq!(unique_name(root, "report.txt"), "report (2).txt");
    assert_eq!(unique_name(root, "Makefile"), "Makefile (1)");

    let temp_file = root.join("delete-me.txt");
    fs::write(&temp_file, b"x").expect("temp file");
    remove_target(&temp_file).expect("remove target file");
    assert!(!temp_file.exists());

    let temp_target_dir = root.join("delete-target-dir");
    fs::create_dir(&temp_target_dir).expect("temp target dir");
    fs::write(temp_target_dir.join("child.txt"), b"x").expect("target child");
    remove_target(&temp_target_dir).expect("remove target dir");
    assert!(!temp_target_dir.exists());

    let temp_source_file = root.join("delete-source-file.txt");
    fs::write(&temp_source_file, b"x").expect("temp source file");
    remove_source(&temp_source_file).expect("remove source file");
    assert!(!temp_source_file.exists());

    let temp_dir = root.join("delete-dir");
    fs::create_dir(&temp_dir).expect("temp dir");
    fs::write(temp_dir.join("child.txt"), b"x").expect("child");
    remove_source(&temp_dir).expect("remove source dir");
    assert!(!temp_dir.exists());
}

#[test]
fn filesystem_helpers_surface_cleanup_errors() {
    let fixture = tempdir().expect("temp dir");
    let missing_target = fixture.path().join("missing-target.txt");
    let target_error = remove_target(&missing_target).expect_err("remove missing target");
    assert!(!target_error.is_empty());

    let missing_source = fixture.path().join("missing-source.txt");
    let source_error = remove_source(&missing_source).expect_err("remove missing source");
    assert!(!source_error.is_empty());
}

/// `remove_target(_)`/`remove_source(_)`'s `is_dir()` branch calls
/// `fs::remove_dir_all`, which fails when the directory's parent forbids
/// unlinking the entry — distinct from the "missing path" case above, which
/// only exercises the `remove_file` (non-directory) branch.
#[cfg(unix)]
#[test]
fn filesystem_helpers_surface_remove_dir_all_errors_for_directories() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = tempdir().expect("temp dir");
    let locked_parent = fixture.path().join("locked-parent");
    fs::create_dir_all(&locked_parent).expect("locked parent");

    let target_dir = locked_parent.join("target-dir");
    fs::create_dir(&target_dir).expect("target dir");
    let source_dir = locked_parent.join("source-dir");
    fs::create_dir(&source_dir).expect("source dir");

    let locked = fs::set_permissions(&locked_parent, fs::Permissions::from_mode(0o555)).is_ok();
    if !locked {
        return;
    }

    let target_error = remove_target(&target_dir);
    let source_error = remove_source(&source_dir);

    fs::set_permissions(&locked_parent, fs::Permissions::from_mode(0o755))
        .expect("restore parent permissions");

    assert!(
        target_error.is_err(),
        "removing a dir needs write access on its parent"
    );
    assert!(
        source_error.is_err(),
        "removing a dir needs write access on its parent"
    );
}
