use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use file_explorer_lib::traversal::{safe_destination, walk, TraversalError, TraversalOptions};
use tempfile::tempdir;

#[test]
fn serial_walk_includes_hidden_files_and_never_follows_directory_links() {
    let fixture = tempdir().expect("fixture");
    let root = fixture.path();
    fs::create_dir(root.join("nested")).expect("nested");
    fs::write(root.join(".visible-to-axopane"), b"hidden").expect("hidden");
    fs::write(root.join("nested/file.txt"), b"file").expect("file");

    #[cfg(unix)]
    std::os::unix::fs::symlink(root, root.join("nested/cycle")).expect("cycle link");

    let entries = walk(
        root,
        TraversalOptions::default(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("walk")
    .collect::<Result<Vec<_>, _>>()
    .expect("entries");
    let names = entries
        .iter()
        .map(|entry| {
            entry
                .path
                .strip_prefix(root)
                .expect("relative")
                .to_path_buf()
        })
        .collect::<Vec<_>>();
    assert!(names
        .iter()
        .any(|path| path == std::path::Path::new(".visible-to-axopane")));
    assert!(names
        .iter()
        .any(|path| path == std::path::Path::new("nested/file.txt")));
    #[cfg(unix)]
    assert!(names
        .iter()
        .any(|path| path == std::path::Path::new("nested/cycle")));
    #[cfg(unix)]
    assert!(!names
        .iter()
        .any(|path| path.starts_with("nested/cycle/nested")));
}

#[test]
fn cancelled_walk_stops_deterministically() {
    let fixture = tempdir().expect("fixture");
    fs::write(fixture.path().join("file"), b"file").expect("file");
    let cancel = Arc::new(AtomicBool::new(true));
    let mut entries = walk(fixture.path(), TraversalOptions::default(), cancel).expect("walk");
    assert!(matches!(
        entries.next(),
        Some(Err(TraversalError::Cancelled))
    ));
}

#[test]
fn safe_destination_rejects_parent_and_existing_link_escapes() {
    let fixture = tempdir().expect("fixture");
    let root = fixture.path().join("destination");
    fs::create_dir(&root).expect("destination");
    assert!(matches!(
        safe_destination(&root, std::path::Path::new("../outside")),
        Err(TraversalError::DestinationEscape(_))
    ));
    assert!(matches!(
        safe_destination(&root, std::path::Path::new("nested/file")),
        Ok(path) if path == root.join("nested/file")
    ));

    #[cfg(unix)]
    {
        let outside = fixture.path().join("outside");
        fs::create_dir(&outside).expect("outside");
        std::os::unix::fs::symlink(&outside, root.join("linked")).expect("link");
        assert!(matches!(
            safe_destination(&root, std::path::Path::new("linked/file")),
            Err(TraversalError::DestinationEscape(_))
        ));
    }
}

#[test]
fn missing_roots_map_to_a_path_aware_error() {
    let fixture = tempdir().expect("fixture");
    let missing = fixture.path().join("missing");
    assert!(
        matches!(walk(&missing, TraversalOptions::default(), Arc::new(AtomicBool::new(false))), Err(TraversalError::Io(path, _)) if path == missing)
    );
}
