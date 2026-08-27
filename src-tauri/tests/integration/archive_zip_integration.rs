use file_explorer_lib::ops::{OpItem, OpKind, OpStatus, OpsService, StartOpRequest};
use file_explorer_lib::volumes::VolumeInfo;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use zip::ZipArchive;

fn volume(root: &str) -> VolumeInfo {
    VolumeInfo {
        mount_root: root.to_string(),
        label: "fixture".to_string(),
        total_bytes: 1,
        free_bytes: 1,
        is_network: false,
        is_removable: false,
    }
}

#[test]
fn archive_work_is_queued_and_reports_completion_through_ops() {
    let fixture = tempdir().expect("temp dir");
    let source = fixture.path().join("source.txt");
    let archive_dir = fixture.path().join("archives");
    fs::create_dir(&archive_dir).expect("archive dir");
    fs::write(&source, b"queued archive").expect("source");

    let service = OpsService::new(Duration::from_secs(5));
    service.set_volumes(vec![volume(&fixture.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Compress,
        destination_dir: archive_dir
            .join("source.zip")
            .to_string_lossy()
            .into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "source.txt".to_string(),
            size_bytes: 0,
        }],
    });
    assert!(service
        .snapshot()
        .iter()
        .any(|entry| entry.progress.operation_id == id));

    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if service.snapshot().iter().any(|entry| {
            entry.progress.operation_id == id && entry.progress.status == OpStatus::Completed
        }) {
            break;
        }
        thread::yield_now();
    }
    assert!(archive_dir.join("source.zip").exists());
    assert!(service
        .snapshot()
        .iter()
        .any(|entry| entry.progress.operation_id == id
            && entry.progress.status == OpStatus::Completed));
}

/// Regression test for the `crate::traversal`-backed rewrite of
/// `append_archive_path`: a source tree with more than one level of nesting
/// (and, on unix, a symlink member) must still compress correctly through
/// the flat walk that replaced the old per-directory `fs::read_dir`
/// recursion. Asserts every expected member path/content lands in the
/// resulting zip, including a nested subdirectory's file.
#[test]
fn compresses_a_multi_level_nested_tree_through_the_queue() {
    let fixture = tempdir().expect("temp dir");
    let source = fixture.path().join("payload");
    let archive_dir = fixture.path().join("archives");
    fs::create_dir(&archive_dir).expect("archive dir");

    fs::create_dir_all(source.join("level1/level2")).expect("nested dirs");
    fs::write(source.join("root.txt"), b"root content").expect("root file");
    fs::write(source.join("level1/mid.txt"), b"mid content").expect("mid file");
    fs::write(
        source.join("level1/level2/deep.txt"),
        b"deep nested content",
    )
    .expect("deep file");

    #[cfg(unix)]
    std::os::unix::fs::symlink("mid.txt", source.join("level1/link-to-mid.txt"))
        .expect("symlink member");

    let service = OpsService::new(Duration::from_secs(5));
    service.set_volumes(vec![volume(&fixture.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Compress,
        destination_dir: archive_dir
            .join("payload.zip")
            .to_string_lossy()
            .into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "payload".to_string(),
            size_bytes: 0,
        }],
    });

    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if service.snapshot().iter().any(|entry| {
            entry.progress.operation_id == id && entry.progress.status == OpStatus::Completed
        }) {
            break;
        }
        thread::yield_now();
    }
    assert!(service
        .snapshot()
        .iter()
        .any(|entry| entry.progress.operation_id == id
            && entry.progress.status == OpStatus::Completed));

    let archive_path = archive_dir.join("payload.zip");
    assert!(archive_path.exists());

    let file = fs::File::open(&archive_path).expect("open archive");
    let mut archive = ZipArchive::new(file).expect("read archive");
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("archive entry");
        let name = entry.name().to_string();
        if entry.is_dir() {
            members.insert(name, Vec::new());
            continue;
        }
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents).expect("read entry");
        members.insert(name, contents);
    }

    assert_eq!(
        members.get("payload/root.txt").map(Vec::as_slice),
        Some(b"root content".as_slice())
    );
    assert_eq!(
        members.get("payload/level1/mid.txt").map(Vec::as_slice),
        Some(b"mid content".as_slice())
    );
    assert_eq!(
        members
            .get("payload/level1/level2/deep.txt")
            .map(Vec::as_slice),
        Some(b"deep nested content".as_slice())
    );

    #[cfg(unix)]
    assert_eq!(
        members
            .get("payload/level1/link-to-mid.txt")
            .map(Vec::as_slice),
        Some(b"mid.txt".as_slice()),
        "symlink member stores its link target as content"
    );
}
