use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use file_explorer_lib::ops::manifest::ProgressiveManifest;
use file_explorer_lib::ops::{
    archive_root_name_for_tests, archive_stem_for_item_for_tests, begin_file_progress_for_tests,
    copy_dir_recursive, copy_file_with_progress, copy_path, delete_path_with_progress, move_path,
    process_item_for_tests, ConflictInfo, ConflictResolution, OpItem, OpKind, OpState, OpStatus,
    OpsService, TreeCopy, WorkerContext,
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
        skip: Arc::new(AtomicBool::new(false)),
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

fn worker_context(
    state: OpState,
) -> (
    Arc<Mutex<OpState>>,
    Arc<std::sync::Condvar>,
    WorkerContext<'static>,
) {
    let shared_operation = Arc::new(Mutex::new(state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);

    let shared_operation_ref = Box::leak(Box::new(shared_operation.clone()));
    let resolver_ref = Box::leak(Box::new(resolver.clone()));
    let progress_ref = Box::leak(Box::new(progress));
    let instant_now_ref = Box::leak(Box::new(instant_now));

    let context = WorkerContext {
        shared_operation: shared_operation_ref,
        resolver: resolver_ref,
        progress: progress_ref,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: instant_now_ref,
    };

    (shared_operation, resolver, context)
}

type ConflictEmitter = Arc<dyn Fn(ConflictInfo) + Send + Sync>;

fn single_item_state(
    kind: OpKind,
    destination_dir: PathBuf,
    source: &Path,
    size_bytes: u64,
) -> OpState {
    let name = source
        .file_name()
        .expect("item name")
        .to_string_lossy()
        .into_owned();
    OpState {
        kind,
        destination_dir,
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name,
            size_bytes,
        }],
        ..state()
    }
}

fn without_conflict(state: OpState) -> OpState {
    OpState {
        conflict: None,
        conflict_resolution: None,
        apply_to_all: None,
        rename_to: None,
        ..state
    }
}

fn process_only_item(state: OpState, emitter: Option<ConflictEmitter>) -> Arc<Mutex<OpState>> {
    let (operation, _resolver, context) = worker_context(state);
    let item = operation.lock().expect("operation lock").items[0].clone();
    process_item_for_tests(&context, &item, emitter);
    operation
}

fn process_resolving_conflict(
    state: OpState,
    resolution: ConflictResolution,
    emitter: Option<ConflictEmitter>,
) -> Arc<Mutex<OpState>> {
    let (operation, resolver, context) = worker_context(state);
    let item = operation.lock().expect("operation lock").items[0].clone();
    let deciding_operation = operation.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        deciding_operation
            .lock()
            .expect("operation lock")
            .conflict_resolution = Some(resolution);
        resolver.notify_all();
    });
    process_item_for_tests(&context, &item, emitter);
    operation
}

fn completed_items(operation: &Arc<Mutex<OpState>>) -> u64 {
    operation.lock().expect("operation lock").completed_items
}

fn error_mentions(operation: &Arc<Mutex<OpState>>, fragment: &str) -> bool {
    operation
        .lock()
        .expect("operation lock")
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains(fragment))
}

fn write_conflicting_pair(source: &Path, destination: &Path, incoming: &[u8], existing: &[u8]) {
    std::fs::write(source, incoming).expect("incoming source");
    std::fs::write(destination, existing).expect("existing destination");
}

struct QueuedOp<'a> {
    id: &'a str,
    kind: OpKind,
    destination_dir: PathBuf,
    source: &'a Path,
    size_bytes: u64,
    volume: &'a str,
}

fn queued_state(queued: QueuedOp<'_>) -> OpState {
    OpState {
        id: queued.id.to_string(),
        volumes: HashSet::from([queued.volume.to_string()]),
        status: OpStatus::Pending,
        total_items: 1,
        completed_items: 0,
        total_bytes: queued.size_bytes,
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
        ..without_conflict(single_item_state(
            queued.kind,
            queued.destination_dir,
            queued.source,
            queued.size_bytes,
        ))
    }
}

fn status_of(service: &OpsService, id: &str) -> OpStatus {
    service
        .snapshot()
        .iter()
        .find(|snapshot| snapshot.progress.operation_id == id)
        .expect("operation snapshot")
        .progress
        .status
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

    let shared_operation = Arc::new(Mutex::new(state()));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    copy_file_with_progress(&source_file, &dest_dir.join("copied.txt"), &context)
        .expect("copy file");
    assert_eq!(
        std::fs::read(dest_dir.join("copied.txt")).expect("copied file"),
        b"alpha"
    );

    copy_path(
        &source_file,
        &dest_dir.join("copied-via-path.txt"),
        &context,
    )
    .expect("copy file path");
    assert_eq!(
        std::fs::read(dest_dir.join("copied-via-path.txt")).expect("copied file path"),
        b"alpha"
    );

    copy_path(&source_dir, &dest_dir.join("copied-tree"), &context).expect("copy tree");
    assert_eq!(
        std::fs::read(dest_dir.join("copied-tree").join("nested").join("beta.txt"))
            .expect("copied nested"),
        b"beta"
    );

    move_path(&source_file, &dest_dir.join("moved.txt"), &context).expect("move file");
    assert!(!source_file.exists());
    assert_eq!(
        std::fs::read(dest_dir.join("moved.txt")).expect("moved file"),
        b"alpha"
    );

    let source_dir_for_move = fixture.path().join("source-move");
    std::fs::create_dir(&source_dir_for_move).expect("source move dir");
    std::fs::write(source_dir_for_move.join("gamma.txt"), b"gamma").expect("source move file");
    let moved_dir = dest_dir.join("moved-tree");
    move_path(&source_dir_for_move, &moved_dir, &context).expect("move dir");
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

    let shared_operation = Arc::new(Mutex::new(cancelled_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    copy_file_with_progress(&source_file, &dest_dir.join("cancelled.txt"), &context)
        .expect("cancelled file copy");
    assert_eq!(
        std::fs::metadata(dest_dir.join("cancelled.txt"))
            .expect("cancelled file metadata")
            .len(),
        0
    );

    copy_path(&source_dir, &dest_dir.join("cancelled-tree"), &context)
        .expect("cancelled tree copy");
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
    let shared_operation = Arc::new(Mutex::new(delete_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    delete_path_with_progress(&file, true, &context).expect("delete file");
    delete_path_with_progress(&dir, true, &context).expect("delete dir");

    assert!(!file.exists());
    assert!(!dir.exists());
    assert_eq!(shared_operation.lock().expect("op lock").copied_bytes, 9);
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
    let shared_operation = Arc::new(Mutex::new(delete_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    delete_path_with_progress(&container, true, &context).expect("delete container with symlinks");

    assert!(!container.exists());
    assert!(
        target_dir.join("keep.txt").exists(),
        "link target dir survives"
    );
    assert!(target_file.exists(), "link target file survives");
}

#[test]
fn file_operation_helpers_surface_io_errors_directly() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path();
    let dest_dir = root.join("dest");
    std::fs::create_dir(&dest_dir).expect("dest dir");

    let shared_operation = Arc::new(Mutex::new(state()));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    let missing_file = root.join("missing.txt");
    let copy_error = copy_path(&missing_file, &dest_dir.join("copy.txt"), &context)
        .expect_err("copy missing file");
    assert!(!copy_error.is_empty());

    let move_error = move_path(&missing_file, &dest_dir.join("move.txt"), &context)
        .expect_err("move missing file");
    assert!(!move_error.is_empty());

    let delete_error =
        delete_path_with_progress(&missing_file, false, &context).expect_err("delete missing file");
    assert!(!delete_error.is_empty());

    let missing_dir = root.join("missing-dir");
    let mut visited = HashSet::from([root.to_path_buf()]);
    let tree = TreeCopy {
        root,
        visited: &mut visited,
        add_discovered_totals: false,
        context: &context,
        throttle: None,
        manifest: None,
    };
    let copy_dir_error = copy_dir_recursive(tree, &missing_dir, &dest_dir.join("tree"))
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

    let shared_operation = Arc::new(Mutex::new(state()));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    let mut visited = HashSet::from([source_dir.clone()]);
    let tree = TreeCopy {
        root: &source_dir,
        visited: &mut visited,
        add_discovered_totals: false,
        context: &context,
        throttle: None,
        manifest: None,
    };
    let error =
        copy_dir_recursive(tree, &source_dir, &target).expect_err("target parent is a file");
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
    let shared_operation = Arc::new(Mutex::new(delete_state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);
    let context = WorkerContext {
        shared_operation: &shared_operation,
        resolver: &resolver,
        progress: &progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now: &instant_now,
    };

    if denied {
        let read_dir_error = delete_path_with_progress(&unreadable_dir, false, &context)
            .expect_err("unreadable dir");
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

    let remove_dir_error = delete_path_with_progress(&target_dir, false, &context);
    std::fs::set_permissions(&locked_parent, std::fs::Permissions::from_mode(0o755))
        .expect("restore locked parent permissions");

    assert!(
        remove_dir_error.is_err(),
        "removing the now-empty dir needs write access on its parent"
    );
}

#[test]
fn process_item_test_hook_deletes_an_item() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let delete_source = fixture.path().join("delete.txt");
    std::fs::write(&delete_source, b"delete").expect("delete source");

    let delete_state =
        single_item_state(OpKind::Delete, state().destination_dir, &delete_source, 6);
    let operation = process_only_item(delete_state, None);
    assert!(!delete_source.exists());
    assert_eq!(completed_items(&operation), 1);
}

#[test]
fn process_item_test_hook_compresses_then_extracts_a_folder() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let source_dir = fixture.path().join("payload");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    std::fs::write(source_dir.join("inside.txt"), b"payload").expect("inside");
    let archive_path = fixture.path().join("payload.zip");

    let compress_state = single_item_state(OpKind::Compress, archive_path.clone(), &source_dir, 0);
    let compress_operation = process_only_item(compress_state, None);
    assert!(archive_path.exists());
    assert_eq!(completed_items(&compress_operation), 1);

    let extract_root = fixture.path().join("extract");
    let extract_state = single_item_state(OpKind::Extract, extract_root.clone(), &archive_path, 0);
    let extract_operation = process_only_item(extract_state, None);
    assert_eq!(
        std::fs::read(extract_root.join("payload").join("inside.txt")).expect("extract"),
        b"payload"
    );
    assert_eq!(completed_items(&extract_operation), 1);
}

#[test]
fn process_item_test_hook_refuses_to_move_a_folder_into_itself() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let source_tree = fixture.path().join("tree");
    let nested_dest = source_tree.join("nested");
    std::fs::create_dir_all(&nested_dest).expect("nested");
    std::fs::write(source_tree.join("top.txt"), b"top").expect("top");

    let move_state = single_item_state(OpKind::Move, nested_dest, &source_tree, 3);
    let operation = process_only_item(move_state, None);
    assert!(error_mentions(&operation, "Cannot move"));
}

#[test]
fn process_item_compresses_every_selected_item_into_one_archive() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let folder = fixture.path().join("folder");
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(folder.join("inside.txt"), b"inside").expect("inside");
    let file = fixture.path().join("file.txt");
    std::fs::write(&file, b"file").expect("file");
    let archive_path = fixture.path().join("bundle.zip");

    let mut compress_state = state();
    compress_state.kind = OpKind::Compress;
    compress_state.destination_dir = archive_path.clone();
    compress_state.error_message = None;
    compress_state.items = [&folder, &file]
        .iter()
        .map(|path| OpItem {
            source_path: path.to_string_lossy().into_owned(),
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            size_bytes: 0,
        })
        .collect();
    let (compress_operation, _resolver, context) = worker_context(compress_state);
    let items = compress_operation
        .lock()
        .expect("compress lock")
        .items
        .clone();
    for item in &items {
        process_item_for_tests(&context, item, None);
    }

    let operation = compress_operation.lock().expect("compress lock");
    assert_eq!(operation.error_message, None);
    assert_eq!(operation.completed_items, 2);
    drop(operation);
    let archive =
        zip::ZipArchive::new(std::fs::File::open(&archive_path).expect("open")).expect("zip");
    let mut names: Vec<&str> = archive.file_names().collect();
    names.sort_unstable();
    assert_eq!(names, ["file.txt", "folder/", "folder/inside.txt"]);
}

#[test]
fn ops_service_test_hooks_run_operations_to_their_terminal_status() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let done_source = fixture.path().join("done.txt");
    std::fs::write(&done_source, b"done").expect("done");
    let failed_source = fixture.path().join("missing.txt");
    let cancelled_source = fixture.path().join("cancelled.txt");
    std::fs::write(&cancelled_source, b"cancelled").expect("cancelled");

    let service = OpsService::new(Duration::from_millis(10));
    service.insert_op_for_tests(queued_state(QueuedOp {
        id: "op-1",
        kind: OpKind::Delete,
        destination_dir: PathBuf::new(),
        source: &done_source,
        size_bytes: 4,
        volume: "alpha",
    }));
    service.insert_op_for_tests(queued_state(QueuedOp {
        id: "op-2",
        kind: OpKind::Copy,
        destination_dir: fixture.path().join("dest"),
        source: &failed_source,
        size_bytes: 7,
        volume: "beta",
    }));
    service.insert_op_for_tests(OpState {
        cancel: Arc::new(AtomicBool::new(true)),
        ..queued_state(QueuedOp {
            id: "op-3",
            kind: OpKind::Delete,
            destination_dir: PathBuf::new(),
            source: &cancelled_source,
            size_bytes: 9,
            volume: "gamma",
        })
    });

    service.run_operation_for_tests("op-1");
    service.run_operation_for_tests("op-2");
    service.run_operation_for_tests("op-3");

    assert_eq!(status_of(&service, "op-1"), OpStatus::Completed);
    assert_eq!(status_of(&service, "op-2"), OpStatus::Failed);
    assert_eq!(status_of(&service, "op-3"), OpStatus::Cancelled);
}

#[test]
fn ops_service_release_and_reschedule_starts_the_next_pending_operation() {
    let fixture = tempfile::tempdir().expect("temp dir");
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
        status: OpStatus::Completed,
        completed_items: 1,
        copied_bytes: 4,
        completed_at: Some(Instant::now()),
        ..queued_state(QueuedOp {
            id: "done-op",
            kind: OpKind::Delete,
            destination_dir: PathBuf::new(),
            source: &completed_file,
            size_bytes: 4,
            volume: "delta",
        })
    });
    reschedule_service.insert_op_for_tests(queued_state(QueuedOp {
        id: "pending-op",
        kind: OpKind::Delete,
        destination_dir: PathBuf::new(),
        source: &pending_file,
        size_bytes: 4,
        volume: "epsilon",
    }));

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
    write_conflicting_pair(
        &source_skip,
        &dest.join("skip.txt"),
        b"incoming",
        b"existing",
    );
    let skip_state = without_conflict(single_item_state(
        OpKind::Copy,
        dest.clone(),
        &source_skip,
        8,
    ));
    let skip_op = process_resolving_conflict(skip_state, ConflictResolution::Skip, None);
    assert_eq!(completed_items(&skip_op), 1);
    assert_eq!(
        std::fs::read(dest.join("skip.txt")).expect("skip dest"),
        b"existing"
    );

    let source_replace = fixture.path().join("replace.txt");
    write_conflicting_pair(
        &source_replace,
        &dest.join("replace.txt"),
        b"fresh",
        b"stale",
    );
    let replace_state = without_conflict(single_item_state(
        OpKind::Copy,
        dest.clone(),
        &source_replace,
        5,
    ));
    let replace_operation =
        process_resolving_conflict(replace_state, ConflictResolution::Replace, None);
    assert_eq!(completed_items(&replace_operation), 1);
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

    assert_eq!(status_of(&service, "completed"), OpStatus::Completed);
    assert_eq!(status_of(&service, "active"), OpStatus::Active);
    assert_eq!(status_of(&service, "pending"), OpStatus::Pending);
}

#[test]
fn process_item_test_hook_covers_cancelled_and_error_paths() {
    let fixture = tempfile::tempdir().expect("temp dir");

    let delete_source = fixture.path().join("delete.txt");
    std::fs::write(&delete_source, b"delete").expect("delete source");
    let cancelled_delete = OpState {
        cancel: Arc::new(AtomicBool::new(true)),
        ..single_item_state(OpKind::Delete, state().destination_dir, &delete_source, 6)
    };
    let delete_operation = process_only_item(cancelled_delete, None);
    assert!(delete_source.exists());
    assert_eq!(completed_items(&delete_operation), 0);

    let source_dir = fixture.path().join("source");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    std::fs::write(source_dir.join("inside.txt"), b"payload").expect("inside");
    let bad_compress = single_item_state(
        OpKind::Compress,
        fixture.path().join("archive.txt"),
        &source_dir,
        0,
    );
    assert!(error_mentions(
        &process_only_item(bad_compress, None),
        ".zip"
    ));

    let source_text = fixture.path().join("plain.txt");
    std::fs::write(&source_text, b"plain").expect("plain");
    let bad_extract = single_item_state(
        OpKind::Extract,
        fixture.path().join("extract"),
        &source_text,
        0,
    );
    assert!(error_mentions(
        &process_only_item(bad_extract, None),
        ".zip"
    ));
}

#[test]
fn process_item_test_hook_covers_conflict_emitter_and_apply_to_all() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let dest = fixture.path().join("dest");
    std::fs::create_dir_all(&dest).expect("dest");

    let emitted = Arc::new(Mutex::new(Vec::<ConflictInfo>::new()));
    let source_emit = fixture.path().join("emit.txt");
    write_conflicting_pair(&source_emit, &dest.join("emit.txt"), b"fresh", b"existing");
    let emit_state = without_conflict(single_item_state(
        OpKind::Copy,
        dest.clone(),
        &source_emit,
        5,
    ));
    let emitted_sink = emitted.clone();
    let emitter: ConflictEmitter = Arc::new(move |info| {
        emitted_sink.lock().expect("emitted lock").push(info);
    });
    process_resolving_conflict(emit_state, ConflictResolution::Replace, Some(emitter));
    assert_eq!(emitted.lock().expect("emitted lock").len(), 1);
    assert_eq!(
        std::fs::read(dest.join("emit.txt")).expect("emit dest"),
        b"fresh"
    );

    let source_skip = fixture.path().join("blanket.txt");
    write_conflicting_pair(
        &source_skip,
        &dest.join("blanket.txt"),
        b"skip",
        b"existing",
    );
    let blanket_state = OpState {
        apply_to_all: Some(ConflictResolution::Skip),
        ..without_conflict(single_item_state(
            OpKind::Copy,
            dest.clone(),
            &source_skip,
            0,
        ))
    };
    let blanket_operation = process_only_item(blanket_state, None);
    assert_eq!(completed_items(&blanket_operation), 1);
    assert_eq!(
        std::fs::read(dest.join("blanket.txt")).expect("blanket dest"),
        b"existing"
    );
}

#[test]
fn ops_test_hooks_cover_archive_name_fallback_paths() {
    let state = state();
    let (shared_operation, _resolver, context) = worker_context(state);

    assert_eq!(
        archive_stem_for_item_for_tests(PathBuf::from("/").as_path()),
        "Archive"
    );
    assert_eq!(
        archive_root_name_for_tests(PathBuf::from("/").as_path()),
        PathBuf::from("Archive")
    );

    begin_file_progress_for_tests(&context, PathBuf::from("/").as_path(), 0);
    assert_eq!(
        shared_operation
            .lock()
            .expect("op lock")
            .current_file_name
            .as_deref(),
        Some("/")
    );
}

#[test]
fn test_utils_run_is_a_safe_noop() {
    file_explorer_lib::run();
}

/// Mirrors what `move_path_with_total_discovery`'s cross-device fallback does
/// internally: walk a directory tree with `copy_dir_recursive` while threading
/// a live `ProgressiveManifest`, exactly the pattern `throttle` already uses.
/// Proves discovery genuinely populates the manifest (one entry per file,
/// correct root-relative paths, an accurate `discovered_bytes()` total, and a
/// symlink correctly flagged) rather than the type staying dead scaffolding.
#[test]
fn copy_dir_recursive_populates_a_progressive_manifest_during_discovery() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path().join("source");
    std::fs::create_dir_all(root.join("nested")).expect("source tree");
    std::fs::write(root.join("a.txt"), b"hello").expect("a.txt"); // 5 bytes
    std::fs::write(root.join("nested/b.txt"), b"worldly!").expect("b.txt"); // 8 bytes

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(root.join("a.txt"), root.join("nested/link.txt")).expect("symlink");
    }

    let target = fixture.path().join("dest");
    let (_shared_operation, _resolver, context) = worker_context(state());
    // `copy_path_with_total_discovery` always canonicalizes the *walked*
    // source path itself before recursing (not just the `root` it passes
    // down), so mirror that here: on macOS the temp dir lives under a
    // symlinked prefix (`/tmp` -> `/private/tmp`), and passing the
    // non-canonical `root` as the walk root while `root` is canonical would
    // make every discovered path fail `strip_prefix`.
    let canonical_root = root.canonicalize().expect("canonicalize root");
    let mut visited = HashSet::from([canonical_root.clone()]);
    let mut manifest = ProgressiveManifest::default();

    let tree = TreeCopy {
        root: &canonical_root,
        visited: &mut visited,
        add_discovered_totals: false,
        context: &context,
        throttle: None,
        manifest: Some(&mut manifest),
    };
    copy_dir_recursive(tree, &canonical_root, &target).expect("copy dir recursive");

    let expected_files = if cfg!(unix) { 3 } else { 2 };
    assert_eq!(manifest.entries().len(), expected_files);
    // 5 (a.txt) + 8 (nested/b.txt), plus the symlink's own on-disk length on
    // unix (the link records `symlink_metadata().len()`, i.e. the length of
    // the link itself rather than its target).
    let expected_bytes: u64 = 13
        + manifest
            .entries()
            .iter()
            .find(|entry| entry.is_link)
            .map(|entry| entry.bytes)
            .unwrap_or(0);
    assert_eq!(manifest.discovered_bytes(), expected_bytes);

    let relative_paths: HashSet<PathBuf> = manifest
        .entries()
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect();
    assert!(relative_paths.contains(&PathBuf::from("a.txt")));
    assert!(relative_paths.contains(&PathBuf::from("nested/b.txt")));

    #[cfg(unix)]
    {
        let link_entry = manifest
            .entries()
            .iter()
            .find(|entry| entry.relative_path.as_path() == Path::new("nested/link.txt"))
            .expect("link entry present");
        assert!(link_entry.is_link);
    }

    for entry in manifest.entries() {
        if entry.relative_path.as_path() != Path::new("nested/link.txt") {
            assert!(!entry.is_link);
        }
    }
}
