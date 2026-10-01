use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use file_explorer_lib::ops::{
    process_item_for_tests, ConflictResolution, OpItem, OpKind, OpProgress, OpState, OpStatus,
    WorkerCtx,
};

fn replace_all_state(kind: OpKind, destination_dir: &Path, source: &Path) -> OpState {
    OpState {
        id: "merge-op".to_string(),
        kind,
        destination_dir: destination_dir.to_path_buf(),
        items: vec![OpItem {
            source_path: source.to_string_lossy().into_owned(),
            name: source.file_name().unwrap().to_string_lossy().into_owned(),
            size_bytes: 0,
        }],
        volumes: HashSet::new(),
        status: OpStatus::Active,
        total_items: 1,
        completed_items: 0,
        total_bytes: 0,
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
        skip: Arc::new(AtomicBool::new(false)),
        conflict: None,
        conflict_resolution: None,
        apply_to_all: Some(ConflictResolution::Replace),
        rename_to: None,
    }
}

/// Run the single item of `state` through the worker with Replace preset;
/// returns the recorded error and completed-item count.
fn run(state: OpState) -> (Option<String>, u64) {
    let op_arc: &'static Arc<Mutex<OpState>> = Box::leak(Box::new(Arc::new(Mutex::new(state))));
    let resolver: &'static Arc<Condvar> = Box::leak(Box::new(Arc::new(Condvar::new())));
    let progress: &'static Option<Arc<dyn Fn(OpProgress) + Send + Sync>> =
        Box::leak(Box::new(None));
    let instant_now: &'static Arc<dyn Fn() -> Instant + Send + Sync> = Box::leak(Box::new(
        Arc::new(Instant::now) as Arc<dyn Fn() -> Instant + Send + Sync>,
    ));
    let ctx = WorkerCtx {
        op_arc,
        resolver,
        progress,
        start: Instant::now(),
        rate_window: Duration::from_secs(1),
        instant_now,
    };
    let item = op_arc.lock().unwrap().items[0].clone();
    process_item_for_tests(&ctx, &item, None);
    let guard = op_arc.lock().unwrap();
    (guard.error_message.clone(), guard.completed_items)
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

/// src/A/{top.txt, x/{shared.txt, new.txt, y/deep.txt}}
/// dst/A/{keep.txt, top.txt, x/{shared.txt, only.txt, y/{deep.txt, keep_deep.txt}}}
fn deep_fixture(root: &Path) -> (PathBuf, PathBuf) {
    let source = root.join("src").join("A");
    write(&source.join("top.txt"), "src-top");
    write(&source.join("x/shared.txt"), "src-shared");
    write(&source.join("x/new.txt"), "src-new");
    write(&source.join("x/y/deep.txt"), "src-deep");

    let destination_dir = root.join("dst");
    let target = destination_dir.join("A");
    write(&target.join("keep.txt"), "dst-keep");
    write(&target.join("top.txt"), "dst-top");
    write(&target.join("x/shared.txt"), "dst-shared");
    write(&target.join("x/only.txt"), "dst-only");
    write(&target.join("x/y/deep.txt"), "dst-deep");
    write(&target.join("x/y/keep_deep.txt"), "dst-keep-deep");
    (source, destination_dir)
}

fn assert_deep_merged(target: &Path) {
    assert_eq!(read(&target.join("keep.txt")), "dst-keep");
    assert_eq!(read(&target.join("top.txt")), "src-top");
    assert_eq!(read(&target.join("x/shared.txt")), "src-shared");
    assert_eq!(read(&target.join("x/new.txt")), "src-new");
    assert_eq!(read(&target.join("x/only.txt")), "dst-only");
    assert_eq!(read(&target.join("x/y/deep.txt")), "src-deep");
    assert_eq!(read(&target.join("x/y/keep_deep.txt")), "dst-keep-deep");
}

#[test]
fn replace_copy_of_folder_merges_nested_contents() {
    let fixture = tempfile::tempdir().unwrap();
    let (source, destination_dir) = deep_fixture(fixture.path());

    let (error, completed) = run(replace_all_state(OpKind::Copy, &destination_dir, &source));

    assert_eq!(error, None);
    assert_eq!(completed, 1);
    assert_deep_merged(&destination_dir.join("A"));
    assert_eq!(read(&source.join("x/y/deep.txt")), "src-deep");
}

#[test]
fn replace_move_of_folder_merges_nested_contents_and_removes_source() {
    let fixture = tempfile::tempdir().unwrap();
    let (source, destination_dir) = deep_fixture(fixture.path());

    let (error, completed) = run(replace_all_state(OpKind::Move, &destination_dir, &source));

    assert_eq!(error, None);
    assert_eq!(completed, 1);
    assert_deep_merged(&destination_dir.join("A"));
    assert!(!source.exists());
}

#[test]
fn replace_merge_lets_source_win_file_and_folder_type_clashes() {
    for kind in [OpKind::Copy, OpKind::Move] {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("src").join("A");
        write(&source.join("was_dir"), "src-file");
        write(&source.join("was_file/inner.txt"), "src-inner");
        let destination_dir = fixture.path().join("dst");
        let target = destination_dir.join("A");
        write(&target.join("was_dir/old.txt"), "dst-old");
        write(&target.join("was_file"), "dst-file");

        let (error, _) = run(replace_all_state(kind, &destination_dir, &source));

        assert_eq!(error, None, "{kind:?}");
        assert_eq!(read(&target.join("was_dir")), "src-file");
        assert_eq!(read(&target.join("was_file/inner.txt")), "src-inner");
    }
}

#[test]
fn replace_folder_with_itself_keeps_source_intact() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("A");
    write(&source.join("x/file.txt"), "keep");

    let (error, completed) = run(replace_all_state(OpKind::Copy, fixture.path(), &source));

    assert_eq!(error, None);
    assert_eq!(completed, 1);
    assert_eq!(read(&source.join("x/file.txt")), "keep");
}

#[cfg(unix)]
#[test]
fn replace_merge_replaces_nested_symlink_without_writing_through_it() {
    let fixture = tempfile::tempdir().unwrap();
    let outside = fixture.path().join("outside.txt");
    write(&outside, "outside");
    let source = fixture.path().join("src").join("A");
    write(&source.join("link.txt"), "src-link");
    let destination_dir = fixture.path().join("dst");
    let target = destination_dir.join("A");
    fs::create_dir_all(&target).unwrap();
    std::os::unix::fs::symlink(&outside, target.join("link.txt")).unwrap();

    let (error, _) = run(replace_all_state(OpKind::Copy, &destination_dir, &source));

    assert_eq!(error, None);
    assert_eq!(read(&outside), "outside");
    assert!(!fs::symlink_metadata(target.join("link.txt"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(read(&target.join("link.txt")), "src-link");
}
