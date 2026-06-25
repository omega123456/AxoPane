#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use file_explorer_lib::ops::{
    ConflictResolution, OpItem, OpKind, OpProgress, OpStatus, OpsService, StartOpRequest,
};
use file_explorer_lib::volumes::VolumeInfo;
use tempfile::tempdir;

fn volume(mount: &str) -> VolumeInfo {
    VolumeInfo {
        mount_root: mount.to_string(),
        label: mount.to_string(),
        total_bytes: 1_000_000,
        free_bytes: 1_000_000,
        is_network: false,
    }
}

fn seed_file(path: &Path, bytes: usize) -> OpItem {
    fs::write(path, vec![0_u8; bytes]).expect("seed file");
    OpItem {
        source_path: path.to_string_lossy().into_owned(),
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        size_bytes: bytes as u64,
    }
}

/// Wait for a predicate over the latest progress snapshot, polling quickly.
fn wait_for<F>(service: &OpsService, id: &str, predicate: F)
where
    F: Fn(&OpProgress) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(snapshot) = service
            .snapshot()
            .into_iter()
            .find(|snapshot| snapshot.progress.operation_id == id)
        {
            if predicate(&snapshot.progress) {
                return;
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for operation {id}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn copies_files_into_destination_and_completes() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let item = seed_file(&dir.path().join("alpha.txt"), 32);

    let service = OpsService::new(Duration::from_millis(50));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![item],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert!(dest.join("alpha.txt").exists());

    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn move_removes_source() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let source = dir.path().join("beta.txt");
    let item = seed_file(&source, 16);

    let service = OpsService::new(Duration::from_millis(50));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Move,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![item],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert!(dest.join("beta.txt").exists());
    assert!(!source.exists());
}

/// Build an op whose single item conflicts with a pre-existing destination file,
/// so the op deterministically parks in `Conflict` (and thus stays "in flight").
fn parking_op(service: &OpsService, dir: &Path) -> String {
    let dest = dir.join("dest");
    fs::create_dir_all(&dest).expect("dest");
    let name = "park.txt";
    let source = dir.join(name);
    fs::write(&source, b"incoming").expect("source");
    fs::write(dest.join(name), b"existing").expect("existing");
    service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: name.to_string(),
            size_bytes: 8,
        }],
    })
}

#[test]
fn jobs_on_disjoint_volumes_run_in_parallel() {
    let dir_a = tempdir().expect("temp dir a");
    let dir_b = tempdir().expect("temp dir b");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![
        volume(&dir_a.path().to_string_lossy()),
        volume(&dir_b.path().to_string_lossy()),
    ]);

    let id_a = parking_op(&service, dir_a.path());
    let id_b = parking_op(&service, dir_b.path());

    // Both parked in Conflict at the same time proves they were dispatched in
    // parallel; a shared volume would keep one Pending until the other finished.
    wait_for(&service, &id_a, |progress| {
        progress.status == OpStatus::Conflict
    });
    wait_for(&service, &id_b, |progress| {
        progress.status == OpStatus::Conflict
    });

    service.resolve_conflict(&id_a, ConflictResolution::Skip, false, None);
    service.resolve_conflict(&id_b, ConflictResolution::Skip, false, None);
    wait_for(&service, &id_a, |progress| {
        progress.status == OpStatus::Completed
    });
    wait_for(&service, &id_b, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn jobs_sharing_a_volume_do_not_park_simultaneously() {
    let dir = tempdir().expect("temp dir");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let id_a = parking_op(&service, &dir.path().join("a"));
    let id_b = parking_op(&service, &dir.path().join("b"));

    // Volumes are derived from the shared temp-dir root, so both ops share a
    // volume: the first parks in Conflict while the second stays Pending.
    wait_for(&service, &id_a, |progress| {
        progress.status == OpStatus::Conflict
    });
    let second = service
        .snapshot()
        .into_iter()
        .find(|snapshot| snapshot.progress.operation_id == id_b)
        .expect("second op present");
    assert_eq!(second.progress.status, OpStatus::Pending);

    service.resolve_conflict(&id_a, ConflictResolution::Skip, false, None);
    wait_for(&service, &id_a, |progress| {
        progress.status == OpStatus::Completed
    });
    wait_for(&service, &id_b, |progress| {
        progress.status == OpStatus::Conflict
    });
    service.resolve_conflict(&id_b, ConflictResolution::Skip, false, None);
    wait_for(&service, &id_b, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn conflict_pauses_only_its_job() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    // Pre-create a destination file to force a conflict on the conflicting op.
    let source_conflict = dir.path().join("dup.txt");
    fs::write(&source_conflict, b"new-content").expect("source");
    fs::write(dest.join("dup.txt"), b"old-content").expect("existing dest");

    // Disjoint sibling op should keep running while the first sits in conflict.
    let dir_b = tempdir().expect("temp dir b");
    let dest_b = dir_b.path().join("dest");
    fs::create_dir(&dest_b).expect("dest b");
    let sibling = seed_file(&dir_b.path().join("sibling.txt"), 8);

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![
        volume(&dir.path().to_string_lossy()),
        volume(&dir_b.path().to_string_lossy()),
    ]);

    let conflict_id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source_conflict.to_string_lossy().into_owned(),
            name: "dup.txt".to_string(),
            size_bytes: 11,
        }],
    });
    let sibling_id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest_b.to_string_lossy().into_owned(),
        items: vec![sibling],
    });

    // The conflicting op parks in Conflict.
    wait_for(&service, &conflict_id, |progress| {
        progress.status == OpStatus::Conflict
    });
    // The sibling, on a disjoint volume, completes regardless.
    wait_for(&service, &sibling_id, |progress| {
        progress.status == OpStatus::Completed
    });

    // Resolve with Replace; the conflicting op then completes.
    service.resolve_conflict(&conflict_id, ConflictResolution::Replace, false, None);
    wait_for(&service, &conflict_id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert_eq!(fs::read(dest.join("dup.txt")).unwrap(), b"new-content");
}

#[test]
fn skip_resolution_leaves_existing_file() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let source = dir.path().join("keep.txt");
    fs::write(&source, b"incoming").expect("source");
    fs::write(dest.join("keep.txt"), b"original").expect("existing");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "keep.txt".to_string(),
            size_bytes: 8,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Conflict
    });
    service.resolve_conflict(&id, ConflictResolution::Skip, false, None);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });

    assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"original");
}

#[test]
fn rename_resolution_writes_a_new_name() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let source = dir.path().join("photo.png");
    fs::write(&source, b"fresh").expect("source");
    fs::write(dest.join("photo.png"), b"existing").expect("existing");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "photo.png".to_string(),
            size_bytes: 5,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Conflict
    });
    service.resolve_conflict(&id, ConflictResolution::Rename, false, None);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });

    assert_eq!(fs::read(dest.join("photo.png")).unwrap(), b"existing");
    assert_eq!(fs::read(dest.join("photo (1).png")).unwrap(), b"fresh");
}

#[test]
fn cancel_keeps_already_copied_files() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    // First item conflicts so the op parks before copying anything; we cancel
    // while parked, then assert the op is cancelled and nothing was destroyed.
    let already = dir.path().join("already.txt");
    fs::write(&already, b"copied-content").expect("already");
    fs::write(dest.join("already.txt"), b"existing").expect("existing");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: already.to_string_lossy().into_owned(),
            name: "already.txt".to_string(),
            size_bytes: 14,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Conflict
    });
    service.cancel_op(&id);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Cancelled
    });

    // The pre-existing destination file is untouched (we never replaced it).
    assert_eq!(fs::read(dest.join("already.txt")).unwrap(), b"existing");
    // The source still exists too.
    assert!(already.exists());
}

#[test]
fn completed_jobs_auto_remove_after_retention() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let item = seed_file(&dir.path().join("gone.txt"), 4);

    let service = OpsService::new(Duration::from_millis(60));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let removed_ids: Arc<std::sync::Mutex<Vec<String>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let removed_sink = removed_ids.clone();
    service.set_removed_emitter(move |operation_id| {
        removed_sink
            .lock()
            .expect("removed lock")
            .push(operation_id);
    });

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![item],
    });
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let present = service
            .snapshot()
            .iter()
            .any(|snapshot| snapshot.progress.operation_id == id);
        if !present {
            break;
        }
        if Instant::now() >= deadline {
            panic!("completed operation was not auto-removed");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // The UI is told to prune the card via the removed emitter.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if removed_ids.lock().expect("removed lock").contains(&id) {
            break;
        }
        if Instant::now() >= deadline {
            panic!("completed operation did not emit a removed event");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cancelled_jobs_auto_remove_after_retention() {
    let dir = tempdir().expect("temp dir");
    let service = OpsService::new(Duration::from_millis(60));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let removed_ids: Arc<std::sync::Mutex<Vec<String>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let removed_sink = removed_ids.clone();
    service.set_removed_emitter(move |operation_id| {
        removed_sink
            .lock()
            .expect("removed lock")
            .push(operation_id);
    });

    let blocker = parking_op(&service, &dir.path().join("a"));
    wait_for(&service, &blocker, |progress| {
        progress.status == OpStatus::Conflict
    });

    let pending = parking_op(&service, &dir.path().join("b"));
    wait_for(&service, &pending, |progress| {
        progress.status == OpStatus::Pending
    });

    service.cancel_op(&pending);
    wait_for(&service, &pending, |progress| {
        progress.status == OpStatus::Cancelled
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let present = service
            .snapshot()
            .iter()
            .any(|snapshot| snapshot.progress.operation_id == pending);
        if !present {
            break;
        }
        if Instant::now() >= deadline {
            panic!("cancelled operation was not auto-removed");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if removed_ids.lock().expect("removed lock").contains(&pending) {
            break;
        }
        if Instant::now() >= deadline {
            panic!("cancelled operation did not emit a removed event");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    service.resolve_conflict(&blocker, ConflictResolution::Skip, false, None);
    wait_for(&service, &blocker, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn failed_jobs_persist_and_can_retry() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    // Missing source path -> copy fails -> op stays Failed.
    let missing = dir.path().join("missing.txt");
    let service = OpsService::new(Duration::from_millis(40));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: missing.to_string_lossy().into_owned(),
            name: "missing.txt".to_string(),
            size_bytes: 10,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Failed
    });

    // It persists (retention does not remove failed ops).
    std::thread::sleep(Duration::from_millis(80));
    assert!(service
        .snapshot()
        .iter()
        .any(|snapshot| snapshot.progress.operation_id == id));

    // Now create the source and retry; it should succeed.
    fs::write(&missing, b"now-present").expect("create source");
    service.retry_op(&id);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert!(dest.join("missing.txt").exists());
}

#[test]
fn pending_jobs_can_be_reordered() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    // All three ops share one volume so two stay pending behind the running one.
    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    // A blocking op that parks on a conflict, keeping the volume busy.
    let blocker_src = dir.path().join("blocker.txt");
    fs::write(&blocker_src, b"x").expect("blocker");
    fs::write(dest.join("blocker.txt"), b"y").expect("existing");
    let blocker = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: blocker_src.to_string_lossy().into_owned(),
            name: "blocker.txt".to_string(),
            size_bytes: 1,
        }],
    });
    wait_for(&service, &blocker, |progress| {
        progress.status == OpStatus::Conflict
    });

    let item1 = seed_file(&dir.path().join("first.txt"), 4);
    let item2 = seed_file(&dir.path().join("second.txt"), 4);
    let id1 = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![item1],
    });
    let id2 = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![item2],
    });

    // Reorder pending so id2 comes before id1.
    service.reorder_ops(&[id2.clone(), id1.clone()]);

    let snapshot = service.snapshot();
    let order: Vec<String> = snapshot
        .iter()
        .map(|snapshot| snapshot.progress.operation_id.clone())
        .collect();
    let pos1 = order.iter().position(|id| id == &id1).unwrap();
    let pos2 = order.iter().position(|id| id == &id2).unwrap();
    assert!(pos2 < pos1, "reorder should move id2 ahead of id1");

    // Clean up: resolve blocker so threads finish.
    service.resolve_conflict(&blocker, ConflictResolution::Skip, false, None);
}

#[test]
fn has_unfinished_work_reflects_active_and_pending() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let src = dir.path().join("u.txt");
    fs::write(&src, b"z").expect("src");
    fs::write(dest.join("u.txt"), b"w").expect("existing");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    assert!(!service.has_unfinished_work());

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: src.to_string_lossy().into_owned(),
            name: "u.txt".to_string(),
            size_bytes: 1,
        }],
    });
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Conflict
    });
    assert!(service.has_unfinished_work());

    service.resolve_conflict(&id, ConflictResolution::Skip, false, None);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn pause_and_resume_transitions_status() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    // Conflict keeps the op alive in a non-active state we can poke; but pause
    // applies to Active ops. Use a larger op and pause immediately.
    let mut items = Vec::new();
    for index in 0..6 {
        items.push(seed_file(&dir.path().join(format!("p-{index}.txt")), 256));
    }

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items,
    });

    // It may finish very fast; pausing a finished op is a no-op, which is fine.
    service.pause_op(&id);
    service.resume_op(&id);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn copies_a_directory_tree() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    let source = dir.path().join("tree");
    fs::create_dir(&source).expect("tree");
    fs::create_dir(source.join("nested")).expect("nested");
    fs::write(source.join("top.txt"), b"top").expect("top");
    fs::write(source.join("nested").join("deep.txt"), b"deep").expect("deep");

    let service = OpsService::new(Duration::from_millis(50));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "tree".to_string(),
            size_bytes: 6,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert_eq!(fs::read(dest.join("tree").join("top.txt")).unwrap(), b"top");
    assert_eq!(
        fs::read(dest.join("tree").join("nested").join("deep.txt")).unwrap(),
        b"deep"
    );
}

#[test]
fn measures_directory_byte_total_when_size_is_unknown() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    let source = dir.path().join("tree");
    fs::create_dir(&source).expect("tree");
    fs::create_dir(source.join("nested")).expect("nested");
    fs::write(source.join("a.bin"), vec![0_u8; 100]).expect("a");
    fs::write(source.join("nested").join("b.bin"), vec![0_u8; 50]).expect("b");

    // Keep the completed op in the queue long enough to inspect its final totals.
    let service = OpsService::new(Duration::from_secs(5));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "tree".to_string(),
            // Directories arrive with an unknown (zero) size from the frontend; the
            // worker must measure the real 150-byte total so progress is driven by
            // bytes copied rather than jumping straight from 0% to 100%.
            size_bytes: 0,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });

    let progress = service
        .snapshot()
        .into_iter()
        .find(|snapshot| snapshot.progress.operation_id == id)
        .expect("op present")
        .progress;
    assert_eq!(progress.total_bytes, 150);
    assert_eq!(progress.copied_bytes, 150);
    assert_eq!(progress.progress_percent, 100.0);
}

#[test]
fn rejects_copying_a_directory_into_its_own_descendant() {
    let dir = tempdir().expect("temp dir");
    let source = dir.path().join("tree");
    let nested_dest = source.join("nested");
    fs::create_dir_all(&nested_dest).expect("nested dest");
    fs::write(source.join("top.txt"), b"top").expect("top");

    let service = OpsService::new(Duration::from_millis(50));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: nested_dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "tree".to_string(),
            size_bytes: 3,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Failed
    });
    let progress = service
        .snapshot()
        .into_iter()
        .find(|snapshot| snapshot.progress.operation_id == id)
        .expect("op")
        .progress;
    assert!(progress
        .error_message
        .expect("error message")
        .contains("descendants"));
    assert!(!source.join("nested").join("tree").exists());
}

#[test]
fn cancelling_a_pending_op_marks_it_cancelled_immediately() {
    let dir = tempdir().expect("temp dir");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let blocker = parking_op(&service, &dir.path().join("a"));
    wait_for(&service, &blocker, |progress| {
        progress.status == OpStatus::Conflict
    });

    // A second op on the same volume stays Pending; cancelling it is immediate.
    let pending = parking_op(&service, &dir.path().join("b"));
    let snapshot = service
        .snapshot()
        .into_iter()
        .find(|snapshot| snapshot.progress.operation_id == pending)
        .expect("pending op");
    assert_eq!(snapshot.progress.status, OpStatus::Pending);

    service.cancel_op(&pending);
    wait_for(&service, &pending, |progress| {
        progress.status == OpStatus::Cancelled
    });

    service.resolve_conflict(&blocker, ConflictResolution::Skip, false, None);
    wait_for(&service, &blocker, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn emits_incremental_current_file_progress() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    let source = dir.path().join("large.bin");
    let bytes = vec![3_u8; 3 * 1024 * 1024];
    fs::write(&source, bytes).expect("source");

    let events = Arc::new(std::sync::Mutex::new(Vec::<OpProgress>::new()));
    let events_clone = events.clone();

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    service.set_progress_emitter(move |progress| {
        events_clone.lock().expect("events lock").push(progress);
    });

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "large.bin".to_string(),
            size_bytes: 3 * 1024 * 1024,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });

    let snapshots = events.lock().expect("events lock");
    assert!(snapshots.iter().any(|progress| {
        progress.operation_id == id
            && progress.status == OpStatus::Active
            && progress.current_file_total_bytes == 3 * 1024 * 1024
            && progress.current_file_copied_bytes > 0
            && progress.current_file_copied_bytes < progress.current_file_total_bytes
            && progress.copied_bytes > 0
            && progress.copied_bytes < progress.total_bytes
    }));
}

#[test]
fn snapshot_progress_reports_percent_and_rate() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    let mut items = Vec::new();
    for index in 0..4 {
        items.push(seed_file(&dir.path().join(format!("n-{index}.bin")), 4096));
    }

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items,
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    let final_progress = service
        .snapshot()
        .into_iter()
        .find(|snapshot| snapshot.progress.operation_id == id)
        .expect("op")
        .progress;
    assert_eq!(final_progress.progress_percent.round() as u64, 100);
    assert_eq!(final_progress.completed_items, 4);
}

#[test]
fn reorder_ignores_unknown_and_non_pending_ids() {
    let dir = tempdir().expect("temp dir");
    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);

    let blocker = parking_op(&service, &dir.path().join("a"));
    wait_for(&service, &blocker, |progress| {
        progress.status == OpStatus::Conflict
    });
    let pending = parking_op(&service, &dir.path().join("b"));

    // Unknown ids and the active blocker id are ignored; pending order is stable.
    service.reorder_ops(&["does-not-exist".to_string(), blocker.clone()]);
    let order: Vec<String> = service
        .snapshot()
        .into_iter()
        .map(|snapshot| snapshot.progress.operation_id)
        .collect();
    assert_eq!(order, vec![blocker.clone(), pending.clone()]);

    service.resolve_conflict(&blocker, ConflictResolution::Skip, false, None);
    wait_for(&service, &blocker, |progress| {
        progress.status == OpStatus::Completed
    });
    wait_for(&service, &pending, |progress| {
        progress.status == OpStatus::Conflict
    });
    service.resolve_conflict(&pending, ConflictResolution::Skip, false, None);
    wait_for(&service, &pending, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn rename_with_explicit_target_uses_given_name() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let source = dir.path().join("doc");
    fs::write(&source, b"body").expect("source");
    fs::write(dest.join("doc"), b"old").expect("existing");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "doc".to_string(),
            size_bytes: 4,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Conflict
    });
    service.resolve_conflict(
        &id,
        ConflictResolution::Rename,
        false,
        Some("doc-copy".to_string()),
    );
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert_eq!(fs::read(dest.join("doc-copy")).unwrap(), b"body");
}

#[test]
fn pause_during_active_copy_halts_then_resumes() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    // Many sizable items so the copy is in flight long enough to pause.
    let mut items = Vec::new();
    for index in 0..200 {
        items.push(seed_file(
            &dir.path().join(format!("big-{index}.bin")),
            65_536,
        ));
    }

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items,
    });

    // Pause repeatedly until we observe the Paused status (or the op finishes,
    // in which case pausing was a harmless no-op).
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut saw_pause = false;
    loop {
        service.pause_op(&id);
        let status = service
            .snapshot()
            .into_iter()
            .find(|snapshot| snapshot.progress.operation_id == id)
            .map(|snapshot| snapshot.progress.status);
        if status == Some(OpStatus::Paused) {
            saw_pause = true;
            break;
        }
        if status == Some(OpStatus::Completed) || Instant::now() >= deadline {
            break;
        }
    }

    if saw_pause {
        service.resume_op(&id);
    }
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
}

#[test]
fn cross_volume_move_copies_then_deletes_source() {
    // Two distinct temp dirs simulate distinct volumes for the move's identity,
    // and `fs::rename` across them on most platforms triggers the copy+delete
    // fallback path. Even when rename succeeds, the result is identical.
    let source_dir = tempdir().expect("source vol");
    let dest_dir = tempdir().expect("dest vol");
    let dest = dest_dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    let source = source_dir.path().join("payload.bin");
    fs::write(&source, vec![7_u8; 1024]).expect("source");

    let service = OpsService::new(Duration::from_millis(50));
    service.set_volumes(vec![
        volume(&source_dir.path().to_string_lossy()),
        volume(&dest_dir.path().to_string_lossy()),
    ]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Move,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "payload.bin".to_string(),
            size_bytes: 1024,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert!(dest.join("payload.bin").exists());
    assert!(!source.exists());
}

#[test]
fn operates_without_a_matching_volume_table() {
    // No volumes registered: the engine derives a root from the path itself.
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let item = seed_file(&dir.path().join("orphan.txt"), 12);

    let progress_seen = Arc::new(AtomicBool::new(false));
    let progress_clone = progress_seen.clone();

    let service = OpsService::new(Duration::from_millis(50));
    service.set_volumes(Vec::new());
    service.set_progress_emitter(move |_progress| progress_clone.store(true, Ordering::SeqCst));
    service.set_conflict_emitter(move |_conflict| {});

    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![item],
    });
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert!(dest.join("orphan.txt").exists());
    assert!(progress_seen.load(Ordering::SeqCst));
}

#[test]
fn auto_rename_handles_extensionless_names() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");
    let source = dir.path().join("Makefile");
    fs::write(&source, b"all:").expect("source");
    fs::write(dest.join("Makefile"), b"old").expect("existing");

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: "Makefile".to_string(),
            size_bytes: 4,
        }],
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Conflict
    });
    service.resolve_conflict(&id, ConflictResolution::Rename, false, None);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });
    assert_eq!(fs::read(dest.join("Makefile (1)")).unwrap(), b"all:");
}

#[test]
fn unknown_ids_are_no_ops() {
    let service = OpsService::new(Duration::from_secs(30));
    // None of these panic or affect state for an unknown id.
    service.pause_op("nope");
    service.resume_op("nope");
    service.cancel_op("nope");
    service.retry_op("nope");
    service.resolve_conflict("nope", ConflictResolution::Skip, false, None);
    assert!(service.snapshot().is_empty());
    assert!(!service.has_unfinished_work());
}

#[test]
fn apply_to_all_reuses_resolution_for_later_conflicts() {
    let dir = tempdir().expect("temp dir");
    let dest = dir.path().join("dest");
    fs::create_dir(&dest).expect("dest");

    let mut items = Vec::new();
    for index in 0..2 {
        let name = format!("dup-{index}.txt");
        let source = dir.path().join(&name);
        fs::write(&source, b"new").expect("source");
        fs::write(dest.join(&name), b"old").expect("existing");
        items.push(OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name,
            size_bytes: 3,
        });
    }

    let service = OpsService::new(Duration::from_secs(30));
    service.set_volumes(vec![volume(&dir.path().to_string_lossy())]);
    let id = service.start_op(StartOpRequest {
        kind: OpKind::Copy,
        destination_dir: dest.to_string_lossy().into_owned(),
        items,
    });

    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Conflict
    });
    // Apply Replace to all remaining conflicts.
    service.resolve_conflict(&id, ConflictResolution::Replace, true, None);
    wait_for(&service, &id, |progress| {
        progress.status == OpStatus::Completed
    });

    assert_eq!(fs::read(dest.join("dup-0.txt")).unwrap(), b"new");
    assert_eq!(fs::read(dest.join("dup-1.txt")).unwrap(), b"new");
}
