use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use file_explorer_lib::ops::{
    archive_root_name_for_tests, archive_stem_for_item_for_tests, begin_file_progress_for_tests,
    copy_dir_recursive, copy_file_with_progress, copy_path, delete_path_with_progress, move_path,
    process_item_for_tests, ConflictInfo, ConflictResolution, OpItem, OpKind, OpState, OpStatus,
    OpsService, WorkerCtx,
};
use file_explorer_lib::volumes::VolumeInfo;

fn item(path: &str, name: &str) -> OpItem {
    OpItem {
        source_path: path.to_string(),
        name: name.to_string(),
        size_bytes: 10,
    }
}

fn volume(root: &str) -> VolumeInfo {
    VolumeInfo {
        mount_root: root.to_string(),
        label: root.to_string(),
        total_bytes: 1,
        free_bytes: 1,
        is_network: false,
        is_removable: false,
    }
}

fn state() -> OpState {
    OpState {
        id: "op-1".to_string(),
        kind: OpKind::Copy,
        destination_dir: PathBuf::from("dest"),
        items: vec![item(
            if cfg!(windows) {
                "C:\\src\\alpha.txt"
            } else {
                "/src/alpha.txt"
            },
            "alpha.txt",
        )],
        volumes: HashSet::from(["c:".to_string()]),
        status: OpStatus::Active,
        total_items: 1,
        completed_items: 0,
        total_bytes: 100,
        copied_bytes: 40,
        bytes_per_second: 20,
        eta_seconds: Some(3),
        sample_count: 1,
        rate_sample_at: Some(Instant::now()),
        rate_sample_bytes: 10,
        current_file_name: Some("alpha.txt".to_string()),
        current_file_copied: 5,
        current_file_total: 10,
        error_message: Some("boom".to_string()),
        completed_at: Some(Instant::now()),
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
        conflict: Some(ConflictInfo {
            operation_id: "op-1".to_string(),
            source_path: "C:\\src\\alpha.txt".to_string(),
            destination_path: "C:\\dest\\alpha.txt".to_string(),
            name: "alpha.txt".to_string(),
        }),
        conflict_resolution: Some(ConflictResolution::Replace),
        apply_to_all: Some(ConflictResolution::Skip),
        rename_to: Some("alpha (1).txt".to_string()),
    }
}

fn worker_ctx(
    state: OpState,
) -> (
    Arc<Mutex<OpState>>,
    Arc<std::sync::Condvar>,
    WorkerCtx<'static>,
) {
    let op_arc = Arc::new(Mutex::new(state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);

    let op_arc_ref = Box::leak(Box::new(op_arc.clone()));
    let resolver_ref = Box::leak(Box::new(resolver.clone()));
    let progress_ref = Box::leak(Box::new(progress));
    let instant_now_ref = Box::leak(Box::new(instant_now));

    let ctx = WorkerCtx {
        op_arc: op_arc_ref,
        resolver: resolver_ref,
        progress: progress_ref,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: instant_now_ref,
    };

    (op_arc, resolver, ctx)
}

#[test]
fn op_state_helpers_report_progress_snapshot_and_terminal_status() {
    let mut state = state();
    let progress = state.progress();
    assert_eq!(progress.operation_id, "op-1");
    assert_eq!(
        progress.source_dir,
        if cfg!(windows) { "C:\\src" } else { "/src" }
    );
    assert_eq!(progress.copied_bytes, 45);
    assert!(progress.progress_percent > 0.0);

    let snapshot = state.snapshot();
    assert!(snapshot.conflict.is_some());
    assert!(!state.is_terminal());
    assert_eq!(state.visible_copied_bytes(), 45);

    state.status = OpStatus::Completed;
    assert!(state.is_terminal());

    state.reset_runtime_state();
    assert_eq!(state.completed_items, 0);
    assert_eq!(state.copied_bytes, 0);
    assert_eq!(state.bytes_per_second, 0);
    assert_eq!(state.eta_seconds, None);
    assert_eq!(state.current_file_name, None);
    assert_eq!(state.error_message, None);
    assert_eq!(state.conflict, None);
    assert_eq!(state.conflict_resolution, None);
    assert_eq!(state.apply_to_all, None);
    assert_eq!(state.rename_to, None);
}

#[test]
fn ops_service_helpers_cover_default_emitters_and_volume_mapping() {
    let service = OpsService::default();
    assert!(service.snapshot().is_empty());
    assert!(!service.has_unfinished_work());

    service.set_volumes(vec![volume("c:\\"), volume("d:\\")]);
    let roots = service.volumes_for(&[
        std::path::Path::new("C:\\Users\\Omega\\a.txt"),
        std::path::Path::new("D:\\Docs\\b.txt"),
    ]);
    assert!(roots.contains("c:"));
    assert!(roots.contains("d:"));

    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    service.set_progress_emitter(Arc::new(move |progress| {
        seen_clone
            .lock()
            .expect("seen lock")
            .push(progress.operation_id);
    }));
    service.emit_progress(&state());
    assert_eq!(seen.lock().expect("seen lock").as_slice(), ["op-1"]);

    service.set_conflict_emitter(Arc::new(|_conflict| {}));
    service.set_removed_emitter(Arc::new(|_operation_id| {}));

    service.schedule_auto_remove_for("missing-op");
    std::thread::sleep(Duration::from_millis(10));
    assert!(service.snapshot().is_empty());
}

#[test]
fn file_operation_helpers_copy_and_move_paths_directly() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let source_dir = fixture.path().join("source");
    let dest_dir = fixture.path().join("dest");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    std::fs::create_dir_all(&dest_dir).expect("dest dir");

    let source_file = source_dir.join("alpha.txt");
    std::fs::write(&source_file, b"alpha").expect("source file");
    let nested_dir = source_dir.join("nested");
    std::fs::create_dir(&nested_dir).expect("nested dir");
    std::fs::write(nested_dir.join("beta.txt"), b"beta").expect("nested file");

    let op_arc = Arc::new(Mutex::new(state()));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let ctx = WorkerCtx {
        op_arc: &op_arc,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    copy_file_with_progress(&source_file, &dest_dir.join("copied.txt"), &ctx).expect("copy file");
    assert_eq!(
        std::fs::read(dest_dir.join("copied.txt")).expect("copied file"),
        b"alpha"
    );

    copy_path(&source_file, &dest_dir.join("copied-via-path.txt"), &ctx).expect("copy file path");
    assert_eq!(
        std::fs::read(dest_dir.join("copied-via-path.txt")).expect("copied file path"),
        b"alpha"
    );

    copy_path(&source_dir, &dest_dir.join("copied-tree"), &ctx).expect("copy tree");
    assert_eq!(
        std::fs::read(dest_dir.join("copied-tree").join("nested").join("beta.txt"))
            .expect("copied nested"),
        b"beta"
    );

    move_path(&source_file, &dest_dir.join("moved.txt"), &ctx).expect("move file");
    assert!(!source_file.exists());
    assert_eq!(
        std::fs::read(dest_dir.join("moved.txt")).expect("moved file"),
        b"alpha"
    );

    let source_dir_for_move = fixture.path().join("source-move");
    std::fs::create_dir(&source_dir_for_move).expect("source move dir");
    std::fs::write(source_dir_for_move.join("gamma.txt"), b"gamma").expect("source move file");
    let moved_dir = dest_dir.join("moved-tree");
    move_path(&source_dir_for_move, &moved_dir, &ctx).expect("move dir");
    assert!(!source_dir_for_move.exists());
    assert_eq!(
        std::fs::read(moved_dir.join("gamma.txt")).expect("moved nested file"),
        b"gamma"
    );
}

#[test]
fn file_operation_helpers_respect_cancellation_before_copying() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let source_dir = fixture.path().join("source");
    let dest_dir = fixture.path().join("dest");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    std::fs::create_dir_all(&dest_dir).expect("dest dir");

    let source_file = source_dir.join("alpha.txt");
    std::fs::write(&source_file, b"alpha").expect("source file");
    std::fs::write(source_dir.join("beta.txt"), b"beta").expect("source sibling");

    let cancelled_state = state();
    cancelled_state
        .cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let op_arc = Arc::new(Mutex::new(cancelled_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let ctx = WorkerCtx {
        op_arc: &op_arc,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    copy_file_with_progress(&source_file, &dest_dir.join("cancelled.txt"), &ctx)
        .expect("cancelled file copy");
    assert_eq!(
        std::fs::metadata(dest_dir.join("cancelled.txt"))
            .expect("cancelled file metadata")
            .len(),
        0
    );

    copy_path(&source_dir, &dest_dir.join("cancelled-tree"), &ctx).expect("cancelled tree copy");
    assert!(dest_dir.join("cancelled-tree").exists());
    assert!(!dest_dir.join("cancelled-tree").join("beta.txt").exists());
}

#[test]
fn file_operation_helpers_delete_paths_with_progress_directly() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path();
    let file = root.join("delete-file.txt");
    let dir = root.join("delete-dir");
    std::fs::write(&file, b"alpha").expect("file");
    std::fs::create_dir(&dir).expect("dir");
    std::fs::write(dir.join("child.txt"), b"beta").expect("child");

    let mut delete_state = state();
    delete_state.kind = OpKind::Delete;
    delete_state.total_bytes = 0;
    delete_state.copied_bytes = 0;
    let op_arc = Arc::new(Mutex::new(delete_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let ctx = WorkerCtx {
        op_arc: &op_arc,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    delete_path_with_progress(&file, true, &ctx).expect("delete file");
    delete_path_with_progress(&dir, true, &ctx).expect("delete dir");

    assert!(!file.exists());
    assert!(!dir.exists());
    assert_eq!(op_arc.lock().expect("op lock").copied_bytes, 9);
}

/// Deleting a folder that contains a symlink (to a file or a directory) must
/// remove the link itself without an OS permission error and without
/// touching whatever the link points at. On Windows this specifically
/// exercises `remove_file_or_symlink` choosing `RemoveDirectoryW` over
/// `DeleteFileW` for directory symlinks/junctions — using the wrong API is
/// what previously surfaced as "Access is denied" for folders holding
/// symlinked items even though Explorer could delete them fine.
#[cfg(unix)]
#[test]
fn file_operation_helpers_delete_a_folder_containing_symlinks() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path();
    let target_dir = root.join("target-dir");
    let target_file = root.join("target-file.txt");
    std::fs::create_dir(&target_dir).expect("target dir");
    std::fs::write(target_dir.join("keep.txt"), b"keep").expect("target dir child");
    std::fs::write(&target_file, b"keep-file").expect("target file");

    let container = root.join("container");
    std::fs::create_dir(&container).expect("container dir");
    symlink(&target_dir, container.join("linked-dir")).expect("dir symlink");
    symlink(&target_file, container.join("linked-file")).expect("file symlink");

    let mut delete_state = state();
    delete_state.kind = OpKind::Delete;
    let op_arc = Arc::new(Mutex::new(delete_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let ctx = WorkerCtx {
        op_arc: &op_arc,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    delete_path_with_progress(&container, true, &ctx).expect("delete container with symlinks");

    assert!(!container.exists());
    assert!(target_dir.join("keep.txt").exists(), "link target dir survives");
    assert!(target_file.exists(), "link target file survives");
}

#[test]
fn file_operation_helpers_surface_io_errors_directly() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path();
    let dest_dir = root.join("dest");
    std::fs::create_dir(&dest_dir).expect("dest dir");

    let op_arc = Arc::new(Mutex::new(state()));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let ctx = WorkerCtx {
        op_arc: &op_arc,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    let missing_file = root.join("missing.txt");
    let copy_error =
        copy_path(&missing_file, &dest_dir.join("copy.txt"), &ctx).expect_err("copy missing file");
    assert!(!copy_error.is_empty());

    let move_error =
        move_path(&missing_file, &dest_dir.join("move.txt"), &ctx).expect_err("move missing file");
    assert!(!move_error.is_empty());

    let delete_error =
        delete_path_with_progress(&missing_file, false, &ctx).expect_err("delete missing file");
    assert!(!delete_error.is_empty());

    let missing_dir = root.join("missing-dir");
    let mut visited = HashSet::from([root.to_path_buf()]);
    let copy_dir_error = copy_dir_recursive(
        &missing_dir,
        &dest_dir.join("tree"),
        root,
        &mut visited,
        false,
        &ctx,
        None,
    )
    .expect_err("copy missing dir");
    assert!(!copy_dir_error.is_empty());
}

/// `copy_dir_recursive`'s very first step, `fs::create_dir_all(target)`, has
/// its own error closure distinct from the "missing source" case above (which
/// only exercises the later `fs::read_dir(source)` failure). Making the
/// target's parent a plain file is what makes directory creation itself fail.
#[test]
fn copy_dir_recursive_surfaces_create_dir_all_errors_for_the_target() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path();
    let source_dir = root.join("source");
    std::fs::create_dir_all(&source_dir).expect("source dir");

    let blocker = root.join("blocker");
    std::fs::write(&blocker, b"not a directory").expect("blocker file");
    let target = blocker.join("nested-target");

    let op_arc = Arc::new(Mutex::new(state()));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let ctx = WorkerCtx {
        op_arc: &op_arc,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    let mut visited = HashSet::from([source_dir.clone()]);
    let error = copy_dir_recursive(
        &source_dir,
        &target,
        &source_dir,
        &mut visited,
        false,
        &ctx,
        None,
    )
    .expect_err("target parent is a file");
    assert!(!error.is_empty());
}

#[cfg(unix)]
#[test]
fn delete_path_with_progress_surfaces_read_dir_and_remove_dir_errors_for_directories() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path();

    // `delete_dir_recursive`'s own `fs::read_dir(source)` closure: the
    // directory itself is readable via `symlink_metadata` (which doesn't
    // need execute/read permission) but listing its contents does.
    let unreadable_dir = root.join("unreadable");
    std::fs::create_dir(&unreadable_dir).expect("unreadable dir");
    let denied =
        std::fs::set_permissions(&unreadable_dir, std::fs::Permissions::from_mode(0o000)).is_ok();

    let mut delete_state = state();
    delete_state.kind = OpKind::Delete;
    let op_arc = Arc::new(Mutex::new(delete_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let ctx = WorkerCtx {
        op_arc: &op_arc,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    if denied {
        let read_dir_error =
            delete_path_with_progress(&unreadable_dir, false, &ctx).expect_err("unreadable dir");
        assert!(!read_dir_error.is_empty());
        std::fs::set_permissions(&unreadable_dir, std::fs::Permissions::from_mode(0o755))
            .expect("restore unreadable dir permissions");
    }

    // `delete_dir_recursive`'s final `fs::remove_dir(source)` closure: the
    // directory's own contents are removable, but its parent forbids
    // unlinking the (now-empty) directory itself.
    let locked_parent = root.join("locked-parent");
    std::fs::create_dir(&locked_parent).expect("locked parent");
    let target_dir = locked_parent.join("target-dir");
    std::fs::create_dir(&target_dir).expect("target dir");
    std::fs::write(target_dir.join("child.txt"), b"x").expect("child file");

    let locked =
        std::fs::set_permissions(&locked_parent, std::fs::Permissions::from_mode(0o555)).is_ok();
    if !locked {
        return;
    }

    let remove_dir_error = delete_path_with_progress(&target_dir, false, &ctx);
    std::fs::set_permissions(&locked_parent, std::fs::Permissions::from_mode(0o755))
        .expect("restore locked parent permissions");

    assert!(
        remove_dir_error.is_err(),
        "removing the now-empty dir needs write access on its parent"
    );
}

#[test]
fn process_item_test_hook_covers_queue_private_paths() {
    let fixture = tempfile::tempdir().expect("temp dir");

    let delete_source = fixture.path().join("delete.txt");
    std::fs::write(&delete_source, b"delete").expect("delete source");
    let mut delete_state = state();
    delete_state.kind = OpKind::Delete;
    delete_state.items = vec![OpItem {
        source_path: delete_source.to_string_lossy().into_owned(),
        name: "delete.txt".to_string(),
        size_bytes: 6,
    }];
    let (delete_op, _delete_resolver, delete_ctx) = worker_ctx(delete_state);
    let delete_item = delete_op.lock().expect("delete lock").items[0].clone();
    process_item_for_tests(&delete_ctx, &delete_item, None);
    assert!(!delete_source.exists());
    assert_eq!(delete_op.lock().expect("delete lock").completed_items, 1);

    let source_dir = fixture.path().join("payload");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    std::fs::write(source_dir.join("inside.txt"), b"payload").expect("inside");
    let archive_path = fixture.path().join("payload.zip");
    let mut compress_state = state();
    compress_state.kind = OpKind::Compress;
    compress_state.destination_dir = archive_path.clone();
    compress_state.items = vec![OpItem {
        source_path: source_dir.to_string_lossy().into_owned(),
        name: "payload".to_string(),
        size_bytes: 0,
    }];
    let (compress_op, _compress_resolver, compress_ctx) = worker_ctx(compress_state);
    let compress_item = compress_op.lock().expect("compress lock").items[0].clone();
    process_item_for_tests(&compress_ctx, &compress_item, None);
    assert!(archive_path.exists());
    assert_eq!(
        compress_op.lock().expect("compress lock").completed_items,
        1
    );

    let extract_root = fixture.path().join("extract");
    let mut extract_state = state();
    extract_state.kind = OpKind::Extract;
    extract_state.destination_dir = extract_root.clone();
    extract_state.items = vec![OpItem {
        source_path: archive_path.to_string_lossy().into_owned(),
        name: "payload.zip".to_string(),
        size_bytes: 0,
    }];
    let (extract_op, _extract_resolver, extract_ctx) = worker_ctx(extract_state);
    let extract_item = extract_op.lock().expect("extract lock").items[0].clone();
    process_item_for_tests(&extract_ctx, &extract_item, None);
    assert_eq!(
        std::fs::read(extract_root.join("payload").join("inside.txt")).expect("extract"),
        b"payload"
    );
    assert_eq!(extract_op.lock().expect("extract lock").completed_items, 1);

    let source_tree = fixture.path().join("tree");
    let nested_dest = source_tree.join("nested");
    std::fs::create_dir_all(&nested_dest).expect("nested");
    std::fs::write(source_tree.join("top.txt"), b"top").expect("top");
    let mut move_state = state();
    move_state.kind = OpKind::Move;
    move_state.destination_dir = nested_dest.clone();
    move_state.items = vec![OpItem {
        source_path: source_tree.to_string_lossy().into_owned(),
        name: "tree".to_string(),
        size_bytes: 3,
    }];
    let (move_op, _move_resolver, move_ctx) = worker_ctx(move_state);
    let move_item = move_op.lock().expect("move lock").items[0].clone();
    process_item_for_tests(&move_ctx, &move_item, None);
    assert!(move_op
        .lock()
        .expect("move lock")
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains("Cannot move")));
}

#[test]
fn ops_service_test_hooks_cover_private_worker_paths() {
    let fixture = tempfile::tempdir().expect("temp dir");

    let done_source = fixture.path().join("done.txt");
    std::fs::write(&done_source, b"done").expect("done");
    let failed_source = fixture.path().join("missing.txt");
    let cancelled_source = fixture.path().join("cancelled.txt");
    std::fs::write(&cancelled_source, b"cancelled").expect("cancelled");

    let service = OpsService::new(Duration::from_millis(10));
    service.insert_op_for_tests(OpState {
        id: "op-1".to_string(),
        kind: OpKind::Delete,
        destination_dir: PathBuf::new(),
        items: vec![OpItem {
            source_path: done_source.to_string_lossy().into_owned(),
            name: "done.txt".to_string(),
            size_bytes: 4,
        }],
        volumes: HashSet::from(["alpha".to_string()]),
        status: OpStatus::Pending,
        total_items: 1,
        completed_items: 0,
        total_bytes: 4,
        copied_bytes: 0,
        bytes_per_second: 0,
        eta_seconds: None,
        sample_count: 0,
        rate_sample_at: None,
        rate_sample_bytes: 0,
        current_file_name: None,
        current_file_copied: 0,
        current_file_total: 0,
        error_message: None,
        completed_at: None,
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
        conflict: None,
        conflict_resolution: None,
        apply_to_all: None,
        rename_to: None,
    });
    service.insert_op_for_tests(OpState {
        id: "op-2".to_string(),
        kind: OpKind::Copy,
        destination_dir: fixture.path().join("dest"),
        items: vec![OpItem {
            source_path: failed_source.to_string_lossy().into_owned(),
            name: "missing.txt".to_string(),
            size_bytes: 7,
        }],
        volumes: HashSet::from(["beta".to_string()]),
        status: OpStatus::Pending,
        total_items: 1,
        completed_items: 0,
        total_bytes: 7,
        copied_bytes: 0,
        bytes_per_second: 0,
        eta_seconds: None,
        sample_count: 0,
        rate_sample_at: None,
        rate_sample_bytes: 0,
        current_file_name: None,
        current_file_copied: 0,
        current_file_total: 0,
        error_message: None,
        completed_at: None,
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
        conflict: None,
        conflict_resolution: None,
        apply_to_all: None,
        rename_to: None,
    });
    let cancelled = Arc::new(AtomicBool::new(true));
    service.insert_op_for_tests(OpState {
        id: "op-3".to_string(),
        kind: OpKind::Delete,
        destination_dir: PathBuf::new(),
        items: vec![OpItem {
            source_path: cancelled_source.to_string_lossy().into_owned(),
            name: "cancelled.txt".to_string(),
            size_bytes: 9,
        }],
        volumes: HashSet::from(["gamma".to_string()]),
        status: OpStatus::Pending,
        total_items: 1,
        completed_items: 0,
        total_bytes: 9,
        copied_bytes: 0,
        bytes_per_second: 0,
        eta_seconds: None,
        sample_count: 0,
        rate_sample_at: None,
        rate_sample_bytes: 0,
        current_file_name: None,
        current_file_copied: 0,
        current_file_total: 0,
        error_message: None,
        completed_at: None,
        cancel: cancelled,
        pause: Arc::new(AtomicBool::new(false)),
        conflict: None,
        conflict_resolution: None,
        apply_to_all: None,
        rename_to: None,
    });

    service.run_operation_for_tests("op-1");
    service.run_operation_for_tests("op-2");
    service.run_operation_for_tests("op-3");

    let snapshots = service.snapshot();
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.progress.operation_id == "op-1")
            .expect("op-1")
            .progress
            .status,
        OpStatus::Completed
    );
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.progress.operation_id == "op-2")
            .expect("op-2")
            .progress
            .status,
        OpStatus::Failed
    );
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.progress.operation_id == "op-3")
            .expect("op-3")
            .progress
            .status,
        OpStatus::Cancelled
    );

    let reschedule_service = OpsService::new(Duration::from_millis(10));
    let removed = Arc::new(Mutex::new(Vec::<String>::new()));
    let removed_sink = removed.clone();
    reschedule_service.set_removed_emitter(Arc::new(move |id| {
        removed_sink.lock().expect("removed lock").push(id);
    }));

    let completed_file = fixture.path().join("completed.txt");
    let pending_file = fixture.path().join("pending.txt");
    std::fs::write(&completed_file, b"done").expect("completed");
    std::fs::write(&pending_file, b"wait").expect("pending");
    reschedule_service.insert_op_for_tests(OpState {
        id: "done-op".to_string(),
        kind: OpKind::Delete,
        destination_dir: PathBuf::new(),
        items: vec![OpItem {
            source_path: completed_file.to_string_lossy().into_owned(),
            name: "completed.txt".to_string(),
            size_bytes: 4,
        }],
        volumes: HashSet::from(["delta".to_string()]),
        status: OpStatus::Completed,
        total_items: 1,
        completed_items: 1,
        total_bytes: 4,
        copied_bytes: 4,
        bytes_per_second: 0,
        eta_seconds: None,
        sample_count: 0,
        rate_sample_at: None,
        rate_sample_bytes: 0,
        current_file_name: None,
        current_file_copied: 0,
        current_file_total: 0,
        error_message: None,
        completed_at: Some(Instant::now()),
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
        conflict: None,
        conflict_resolution: None,
        apply_to_all: None,
        rename_to: None,
    });
    reschedule_service.insert_op_for_tests(OpState {
        id: "pending-op".to_string(),
        kind: OpKind::Delete,
        destination_dir: PathBuf::new(),
        items: vec![OpItem {
            source_path: pending_file.to_string_lossy().into_owned(),
            name: "pending.txt".to_string(),
            size_bytes: 4,
        }],
        volumes: HashSet::from(["epsilon".to_string()]),
        status: OpStatus::Pending,
        total_items: 1,
        completed_items: 0,
        total_bytes: 4,
        copied_bytes: 0,
        bytes_per_second: 0,
        eta_seconds: None,
        sample_count: 0,
        rate_sample_at: None,
        rate_sample_bytes: 0,
        current_file_name: None,
        current_file_copied: 0,
        current_file_total: 0,
        error_message: None,
        completed_at: None,
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
        conflict: None,
        conflict_resolution: None,
        apply_to_all: None,
        rename_to: None,
    });

    reschedule_service.release_and_reschedule_for_tests("done-op");
    std::thread::sleep(Duration::from_millis(40));
    assert!(!pending_file.exists());
    assert!(removed
        .lock()
        .expect("removed lock")
        .iter()
        .any(|id| id == "done-op"));
}

#[test]
fn process_item_test_hook_covers_conflict_skip_and_replace_paths() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let dest = fixture.path().join("dest");
    std::fs::create_dir_all(&dest).expect("dest");

    let source_skip = fixture.path().join("skip.txt");
    std::fs::write(&source_skip, b"incoming").expect("source skip");
    std::fs::write(dest.join("skip.txt"), b"existing").expect("dest skip");

    let mut skip_state = state();
    skip_state.kind = OpKind::Copy;
    skip_state.destination_dir = dest.clone();
    skip_state.conflict = None;
    skip_state.conflict_resolution = None;
    skip_state.apply_to_all = None;
    skip_state.rename_to = None;
    skip_state.items = vec![OpItem {
        source_path: source_skip.to_string_lossy().into_owned(),
        name: "skip.txt".to_string(),
        size_bytes: 8,
    }];
    let (skip_op, skip_resolver, skip_ctx) = worker_ctx(skip_state);
    let skip_item = skip_op.lock().expect("skip lock").items[0].clone();
    let skip_signal = skip_resolver.clone();
    let skip_state_arc = skip_op.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        let mut guard = skip_state_arc.lock().expect("skip lock");
        guard.conflict_resolution = Some(ConflictResolution::Skip);
        drop(guard);
        skip_signal.notify_all();
    });
    process_item_for_tests(&skip_ctx, &skip_item, None);
    assert_eq!(skip_op.lock().expect("skip lock").completed_items, 1);
    assert_eq!(
        std::fs::read(dest.join("skip.txt")).expect("skip dest"),
        b"existing"
    );

    let source_replace = fixture.path().join("replace.txt");
    std::fs::write(&source_replace, b"fresh").expect("source replace");
    std::fs::write(dest.join("replace.txt"), b"stale").expect("dest replace");
    let mut replace_state = state();
    replace_state.kind = OpKind::Copy;
    replace_state.destination_dir = dest.clone();
    replace_state.conflict = None;
    replace_state.conflict_resolution = None;
    replace_state.apply_to_all = None;
    replace_state.rename_to = None;
    replace_state.items = vec![OpItem {
        source_path: source_replace.to_string_lossy().into_owned(),
        name: "replace.txt".to_string(),
        size_bytes: 5,
    }];
    let (replace_op, replace_resolver, replace_ctx) = worker_ctx(replace_state);
    let replace_item = replace_op.lock().expect("replace lock").items[0].clone();
    let replace_signal = replace_resolver.clone();
    let replace_state_arc = replace_op.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        let mut guard = replace_state_arc.lock().expect("replace lock");
        guard.conflict_resolution = Some(ConflictResolution::Replace);
        drop(guard);
        replace_signal.notify_all();
    });
    process_item_for_tests(&replace_ctx, &replace_item, None);
    assert_eq!(replace_op.lock().expect("replace lock").completed_items, 1);
    assert_eq!(
        std::fs::read(dest.join("replace.txt")).expect("replace dest"),
        b"fresh"
    );
}

#[test]
fn ops_service_methods_cover_terminal_and_noop_paths() {
    let service = OpsService::new(Duration::from_millis(10));

    service.cancel_op("missing");
    service.retry_op("missing");
    service.resolve_conflict("missing", ConflictResolution::Skip, false, None);
    service.run_operation_for_tests("missing");
    service.release_and_reschedule_for_tests("missing");

    service.insert_op_for_tests(OpState {
        id: "completed".to_string(),
        status: OpStatus::Completed,
        completed_at: Some(Instant::now()),
        ..state()
    });
    service.insert_op_for_tests(OpState {
        id: "active".to_string(),
        status: OpStatus::Active,
        ..state()
    });
    service.insert_op_for_tests(OpState {
        id: "pending".to_string(),
        status: OpStatus::Pending,
        ..state()
    });

    service.cancel_op("completed");
    service.resolve_conflict("active", ConflictResolution::Skip, false, None);
    service.retry_op("pending");

    let snapshots = service.snapshot();
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.progress.operation_id == "completed")
            .expect("completed")
            .progress
            .status,
        OpStatus::Completed
    );
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.progress.operation_id == "active")
            .expect("active")
            .progress
            .status,
        OpStatus::Active
    );
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.progress.operation_id == "pending")
            .expect("pending")
            .progress
            .status,
        OpStatus::Pending
    );
}

#[test]
fn process_item_test_hook_covers_cancelled_and_error_paths() {
    let fixture = tempfile::tempdir().expect("temp dir");

    let delete_source = fixture.path().join("delete.txt");
    std::fs::write(&delete_source, b"delete").expect("delete source");
    let mut cancelled_delete = state();
    cancelled_delete.kind = OpKind::Delete;
    cancelled_delete.cancel = Arc::new(AtomicBool::new(true));
    cancelled_delete.items = vec![OpItem {
        source_path: delete_source.to_string_lossy().into_owned(),
        name: "delete.txt".to_string(),
        size_bytes: 6,
    }];
    let (delete_op, _delete_resolver, delete_ctx) = worker_ctx(cancelled_delete);
    let delete_item = delete_op.lock().expect("delete lock").items[0].clone();
    process_item_for_tests(&delete_ctx, &delete_item, None);
    assert!(delete_source.exists());
    assert_eq!(delete_op.lock().expect("delete lock").completed_items, 0);

    let source_dir = fixture.path().join("source");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    std::fs::write(source_dir.join("inside.txt"), b"payload").expect("inside");

    let mut bad_compress = state();
    bad_compress.kind = OpKind::Compress;
    bad_compress.destination_dir = fixture.path().join("archive.txt");
    bad_compress.items = vec![OpItem {
        source_path: source_dir.to_string_lossy().into_owned(),
        name: "source".to_string(),
        size_bytes: 0,
    }];
    let (compress_op, _compress_resolver, compress_ctx) = worker_ctx(bad_compress);
    let compress_item = compress_op.lock().expect("compress lock").items[0].clone();
    process_item_for_tests(&compress_ctx, &compress_item, None);
    assert!(compress_op
        .lock()
        .expect("compress lock")
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains(".zip")));

    let source_text = fixture.path().join("plain.txt");
    std::fs::write(&source_text, b"plain").expect("plain");
    let mut bad_extract = state();
    bad_extract.kind = OpKind::Extract;
    bad_extract.destination_dir = fixture.path().join("extract");
    bad_extract.items = vec![OpItem {
        source_path: source_text.to_string_lossy().into_owned(),
        name: "plain.txt".to_string(),
        size_bytes: 0,
    }];
    let (extract_op, _extract_resolver, extract_ctx) = worker_ctx(bad_extract);
    let extract_item = extract_op.lock().expect("extract lock").items[0].clone();
    process_item_for_tests(&extract_ctx, &extract_item, None);
    assert!(extract_op
        .lock()
        .expect("extract lock")
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains(".zip")));
}

#[test]
fn process_item_test_hook_covers_conflict_emitter_and_apply_to_all() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let dest = fixture.path().join("dest");
    std::fs::create_dir_all(&dest).expect("dest");

    let emitted = Arc::new(Mutex::new(Vec::<ConflictInfo>::new()));
    let source_emit = fixture.path().join("emit.txt");
    std::fs::write(&source_emit, b"fresh").expect("source");
    std::fs::write(dest.join("emit.txt"), b"existing").expect("dest");

    let mut emit_state = state();
    emit_state.kind = OpKind::Copy;
    emit_state.destination_dir = dest.clone();
    emit_state.conflict = None;
    emit_state.conflict_resolution = None;
    emit_state.apply_to_all = None;
    emit_state.rename_to = None;
    emit_state.items = vec![OpItem {
        source_path: source_emit.to_string_lossy().into_owned(),
        name: "emit.txt".to_string(),
        size_bytes: 5,
    }];
    let (emit_op, emit_resolver, emit_ctx) = worker_ctx(emit_state);
    let emit_item = emit_op.lock().expect("emit lock").items[0].clone();
    let emit_signal = emit_resolver.clone();
    let emit_state_arc = emit_op.clone();
    let emitted_sink = emitted.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        let mut guard = emit_state_arc.lock().expect("emit lock");
        guard.conflict_resolution = Some(ConflictResolution::Replace);
        drop(guard);
        emit_signal.notify_all();
    });
    process_item_for_tests(
        &emit_ctx,
        &emit_item,
        Some(Arc::new(move |info| {
            emitted_sink.lock().expect("emitted lock").push(info);
        })),
    );
    assert_eq!(emitted.lock().expect("emitted lock").len(), 1);
    assert_eq!(
        std::fs::read(dest.join("emit.txt")).expect("emit dest"),
        b"fresh"
    );

    let source_skip = fixture.path().join("blanket.txt");
    std::fs::write(&source_skip, b"skip").expect("blanket source");
    std::fs::write(dest.join("blanket.txt"), b"existing").expect("blanket dest");
    let mut blanket_state = state();
    blanket_state.kind = OpKind::Copy;
    blanket_state.destination_dir = dest.clone();
    blanket_state.apply_to_all = Some(ConflictResolution::Skip);
    blanket_state.conflict = None;
    blanket_state.conflict_resolution = None;
    blanket_state.rename_to = None;
    blanket_state.items = vec![OpItem {
        source_path: source_skip.to_string_lossy().into_owned(),
        name: "blanket.txt".to_string(),
        size_bytes: 0,
    }];
    let (blanket_op, _blanket_resolver, blanket_ctx) = worker_ctx(blanket_state);
    let blanket_item = blanket_op.lock().expect("blanket lock").items[0].clone();
    process_item_for_tests(&blanket_ctx, &blanket_item, None);
    assert_eq!(blanket_op.lock().expect("blanket lock").completed_items, 1);
    assert_eq!(
        std::fs::read(dest.join("blanket.txt")).expect("blanket dest"),
        b"existing"
    );
}

#[test]
fn ops_test_hooks_cover_archive_name_fallback_paths() {
    let state = state();
    let (op_arc, _resolver, ctx) = worker_ctx(state);

    assert_eq!(
        archive_stem_for_item_for_tests(PathBuf::from("/").as_path()),
        "Archive"
    );
    assert_eq!(
        archive_root_name_for_tests(PathBuf::from("/").as_path()),
        PathBuf::from("Archive")
    );

    begin_file_progress_for_tests(&ctx, PathBuf::from("/").as_path(), 0);
    assert_eq!(
        op_arc.lock().expect("op lock").current_file_name.as_deref(),
        Some("/")
    );
}

#[test]
fn test_utils_run_is_a_safe_noop() {
    file_explorer_lib::run();
}
