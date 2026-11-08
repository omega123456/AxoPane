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

fn nested_payload(root: &std::path::Path) -> std::path::PathBuf {
    let source = root.join("payload");
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
    source
}

fn is_completed(service: &OpsService, id: &str) -> bool {
    service.snapshot().iter().any(|entry| {
        entry.progress.operation_id == id && entry.progress.status == OpStatus::Completed
    })
}

fn wait_for_completion(service: &OpsService, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && !is_completed(service, id) {
        thread::yield_now();
    }
}

fn archive_members(archive_path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    let file = fs::File::open(archive_path).expect("open archive");
    let mut archive = ZipArchive::new(file).expect("read archive");
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("archive entry");
        let name = entry.name().to_string();
        let mut contents = Vec::new();
        if !entry.is_dir() {
            entry.read_to_end(&mut contents).expect("read entry");
        }
        members.insert(name, contents);
    }
    members
}

fn member<'a>(members: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Option<&'a [u8]> {
    members.get(name).map(Vec::as_slice)
}

#[test]
fn compresses_a_multi_level_nested_tree_through_the_queue() {
    let fixture = tempdir().expect("temp dir");
    let source = nested_payload(fixture.path());
    let archive_dir = fixture.path().join("archives");
    fs::create_dir(&archive_dir).expect("archive dir");

    let service = OpsService::new(Duration::from_secs(5));
    service.set_volumes(vec![volume(&fixture.path().to_string_lossy())]);
    let archive_path = archive_dir.join("payload.zip");
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Compress,
        destination_dir: archive_path.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "payload".to_string(),
            size_bytes: 0,
        }],
    });
    wait_for_completion(&service, &id);
    assert!(is_completed(&service, &id));
    assert!(archive_path.exists());

    let members = archive_members(&archive_path);
    assert_eq!(
        member(&members, "payload/root.txt"),
        Some(b"root content".as_slice())
    );
    assert_eq!(
        member(&members, "payload/level1/mid.txt"),
        Some(b"mid content".as_slice())
    );
    assert_eq!(
        member(&members, "payload/level1/level2/deep.txt"),
        Some(b"deep nested content".as_slice())
    );

    #[cfg(unix)]
    assert_eq!(
        member(&members, "payload/level1/link-to-mid.txt"),
        Some(b"mid.txt".as_slice()),
        "symlink member stores its link target as content"
    );
}
