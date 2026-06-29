use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use file_explorer_lib::ops::{
    copy_dir_recursive, copy_file_with_progress, copy_path, delete_path_with_progress, move_path,
    ConflictInfo, ConflictResolution, OpItem, OpKind, OpState, OpStatus, OpsService, WorkerCtx,
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
    )
    .expect_err("copy missing dir");
    assert!(!copy_dir_error.is_empty());
}

#[test]
fn test_utils_run_is_a_safe_noop() {
    file_explorer_lib::run();
}
