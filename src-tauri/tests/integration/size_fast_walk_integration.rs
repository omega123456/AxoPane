use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::size::fast_walk::{
    helper_threads_for_tests, list_dir, subtree_records, subtree_total, DirRecord, Qos,
    WalkOptions, INDEX_EXCLUDED_ROOTS, MAX_HELPER_THREADS, MAX_JOB_THREADS,
};
use file_explorer_lib::size::index::platform::DATA_VOLUME_PREFIX;
use file_explorer_lib::size::manual::{calculate, ManualSizeError};
use file_explorer_lib::traversal::{walk, TraversalOptions};
use tempfile::tempdir;

fn options() -> WalkOptions {
    WalkOptions::new(Arc::new(AtomicBool::new(false)), None)
}

/// The previous manual-size algorithm: serial shared walker plus one
/// `symlink_metadata` per entry, skipping symlinks and directories.
fn previous_total(root: &Path) -> u64 {
    walk(
        root,
        TraversalOptions::default(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("walk")
    .map(|entry| fs::symlink_metadata(entry.expect("entry").path).expect("metadata"))
    .filter(|metadata| !metadata.is_dir() && !metadata.file_type().is_symlink())
    .map(|metadata| metadata.len())
    .sum()
}

/// Nested folders, empty folders, zero-byte files, symlinks to a file and to
/// a folder, and one directory wide enough to need several 512-byte buffer
/// fills (every entry takes at least 48 bytes).
fn seed_tree(root: &Path) {
    fs::create_dir_all(root.join("a/b/c")).expect("nested");
    fs::create_dir_all(root.join("empty/also-empty")).expect("empty");
    fs::create_dir(root.join("wide")).expect("wide");
    fs::write(root.join("top.bin"), vec![1_u8; 1000]).expect("top");
    fs::write(root.join("a/one.bin"), vec![1_u8; 10]).expect("one");
    fs::write(root.join("a/b/two.bin"), vec![1_u8; 200]).expect("two");
    fs::write(root.join("a/b/c/three.bin"), vec![1_u8; 3000]).expect("three");
    fs::write(root.join("a/zero.bin"), b"").expect("zero");
    for index in 0..WIDE_FILES {
        fs::write(
            root.join(format!("wide/file-{index:02}.bin")),
            vec![1_u8; index],
        )
        .expect("wide");
    }
    symlink(root.join("top.bin"), root.join("link-to-file")).expect("file link");
    symlink(root.join("a"), root.join("link-to-dir")).expect("dir link");
}

const WIDE_FILES: usize = 20;

fn records(root: &Path, options: &WalkOptions) -> HashMap<PathBuf, DirRecord> {
    let collected = Mutex::new(Vec::new());
    subtree_records(root, options, |record| {
        collected.lock().expect("records").push(record);
    })
    .expect("records walk");
    let collected = collected.into_inner().expect("records");
    let by_path: HashMap<_, _> = collected
        .iter()
        .map(|record| (record.path.clone(), record.clone()))
        .collect();
    assert_eq!(by_path.len(), collected.len(), "one record per directory");
    by_path
}

#[test]
fn subtree_total_matches_the_previous_walker_with_multi_fill_buffers() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);
    let expected = previous_total(root);
    assert_eq!(
        expected,
        1000 + 10 + 200 + 3000 + (0..WIDE_FILES as u64).sum::<u64>()
    );

    for (threads, buffer_size) in [
        (1, 512),
        (MAX_JOB_THREADS, 512),
        (MAX_JOB_THREADS, 128 * 1024),
    ] {
        let mut options = options();
        options.threads = threads;
        options.qos = Qos::Background;
        options.hooks.buffer_size = buffer_size;
        assert_eq!(subtree_total(root, &options).expect("total"), expected);
    }

    let manual = calculate(
        root,
        &Arc::new(AtomicBool::new(false)),
        Duration::from_secs(5),
    );
    assert_eq!(manual.expect("manual"), expected);
}

#[test]
fn manual_sizing_preserves_root_edge_cases() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);
    let cancel = Arc::new(AtomicBool::new(false));
    let timeout = Duration::from_secs(5);

    assert_eq!(
        calculate(&root.join("top.bin"), &cancel, timeout).expect("file"),
        0
    );
    // macOS sizes a link as its target; a link to a file is a file root.
    assert_eq!(
        calculate(&root.join("link-to-dir"), &cancel, timeout).expect("link"),
        10 + 200 + 3000
    );
    assert_eq!(
        calculate(&root.join("link-to-file"), &cancel, timeout).expect("file link"),
        0
    );
    symlink(root.join("nowhere"), root.join("dangling")).expect("dangling");
    symlink(root.join("loop-b"), root.join("loop-a")).expect("loop a");
    symlink(root.join("loop-a"), root.join("loop-b")).expect("loop b");
    for broken in ["missing", "dangling", "loop-a"] {
        assert!(
            matches!(
                calculate(&root.join(broken), &cancel, timeout),
                Err(ManualSizeError::Io(_))
            ),
            "{broken}"
        );
    }
    assert!(matches!(
        calculate(root, &cancel, Duration::ZERO),
        Err(ManualSizeError::Timeout)
    ));
    assert!(matches!(
        calculate(root, &Arc::new(AtomicBool::new(true)), timeout),
        Err(ManualSizeError::Cancelled)
    ));
}

#[test]
fn cancellation_mid_read_and_an_expired_deadline_stop_the_walk() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);

    let mut cancelling = options();
    let cancel = Arc::clone(&cancelling.cancel);
    cancelling.hooks.before_dir_open = Some(Arc::new(move |_dir: &Path| {
        cancel.store(true, Ordering::Relaxed);
    }));
    assert!(matches!(
        subtree_total(root, &cancelling),
        Err(ManualSizeError::Cancelled)
    ));
    assert!(matches!(
        list_dir(root, &cancelling),
        Err(ManualSizeError::Cancelled)
    ));

    let expired = WalkOptions::new(Arc::new(AtomicBool::new(false)), Some(Instant::now()));
    assert!(matches!(
        subtree_total(root, &expired),
        Err(ManualSizeError::Timeout)
    ));
}

#[test]
fn vanished_directories_are_skipped_without_failing() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);
    let expected = previous_total(root) - 200 - 3000;

    let mut options = options();
    let doomed = root.join("a/b");
    options.hooks.before_dir_open = Some(Arc::new(move |dir: &Path| {
        if dir == doomed && dir.exists() {
            fs::remove_dir_all(dir).expect("vanish");
        }
    }));
    assert_eq!(subtree_total(root, &options).expect("total"), expected);
    assert!(!records(root, &options).contains_key(&root.join("a/b")));

    assert!(matches!(
        list_dir(&root.join("a/b"), &options),
        Err(ManualSizeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound
    ));
}

#[test]
fn per_directory_records_carry_direct_bytes_and_sum_to_the_total() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);
    let mut options = options();
    options.hooks.buffer_size = 512;

    let by_path = records(root, &options);
    let expected_dirs = ["", "a", "a/b", "a/b/c", "empty", "empty/also-empty", "wide"];
    assert_eq!(by_path.len(), expected_dirs.len());
    let direct = |relative: &str| by_path[&root.join(relative)].direct_bytes;
    assert_eq!(direct(""), 1000);
    assert_eq!(direct("a"), 10);
    assert_eq!(direct("a/b"), 200);
    assert_eq!(direct("a/b/c"), 3000);
    assert_eq!(direct("empty"), 0);
    assert_eq!(direct("wide"), (0..WIDE_FILES as u64).sum::<u64>());
    // Files only: symlinks and subdirectories are not counted.
    let files = |relative: &str| by_path[&root.join(relative)].file_count;
    assert_eq!(files(""), 1);
    assert_eq!(files("a"), 2);
    assert_eq!(files("a/b/c"), 1);
    assert_eq!(files("empty"), 0);
    assert_eq!(files("wide"), WIDE_FILES as u64);
    assert!(by_path.values().all(|record| !record.denied));
    assert_eq!(
        by_path
            .values()
            .map(|record| record.direct_bytes)
            .sum::<u64>(),
        subtree_total(root, &options).expect("total")
    );
}

#[test]
fn index_walks_stop_at_exclusions_and_device_boundaries() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);
    fs::create_dir_all(root.join("Volumes/External/deep")).expect("volumes");
    fs::write(root.join("Volumes/External/deep/big.bin"), vec![1_u8; 5000]).expect("big");

    let index = options().with_index_boundaries();
    assert!(index.stay_on_device);
    assert_eq!(
        INDEX_EXCLUDED_ROOTS,
        ["/System/Volumes", "/Volumes", "/dev"]
    );
    assert!(INDEX_EXCLUDED_ROOTS
        .iter()
        .all(|excluded| index.excluded.contains(Path::new(excluded))));

    // Fake devices: the walk root on 1, `a/b` on 2, `a/b/c` on 3, and the
    // Data volume on `data_dev`. Only the root's and the Data volume's
    // devices are allowed.
    let walk_with = |data_dev: u64| {
        let mut options = options();
        options.stay_on_device = true;
        options.excluded.insert(root.join("Volumes"));
        let (second, third) = (root.join("a/b"), root.join("a/b/c"));
        options.hooks.device_of = Some(Arc::new(move |dir: &Path, _dev: u64| {
            if dir == Path::new(DATA_VOLUME_PREFIX) {
                data_dev
            } else if dir == second {
                2
            } else if dir == third {
                3
            } else {
                1
            }
        }));
        (records(root, &options), subtree_total(root, &options))
    };

    // Same device for the root and the Data volume (macOS 14+).
    let (by_path, total) = walk_with(1);
    for skipped in ["Volumes", "Volumes/External", "a/b", "a/b/c"] {
        assert!(
            !by_path.contains_key(&root.join(skipped)),
            "{skipped} walked"
        );
    }
    assert!(by_path.contains_key(&root.join("a")));
    assert_eq!(
        total.expect("total"),
        previous_total(root) - 5000 - 200 - 3000
    );

    // Two allowed devices (macOS 10.15–13): the Data volume's is walked, a
    // third one is still stopped.
    let (by_path, total) = walk_with(2);
    assert!(by_path.contains_key(&root.join("a/b")));
    assert!(!by_path.contains_key(&root.join("a/b/c")));
    assert_eq!(total.expect("total"), previous_total(root) - 5000 - 3000);
}

#[test]
fn single_directory_listing_reports_bytes_subdirs_and_denial() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);
    let mut options = options();
    options.hooks.buffer_size = 512;

    let listing = list_dir(root, &options).expect("listing");
    assert_eq!(listing.direct_bytes, 1000);
    assert_eq!(listing.file_count, 1);
    assert!(!listing.denied);
    let mut subdirs = listing.subdirs.clone();
    subdirs.sort();
    assert_eq!(subdirs, ["a", "empty", "wide"]);
    assert_eq!(
        list_dir(&root.join("wide"), &options)
            .expect("wide")
            .subdirs
            .len(),
        0
    );

    let locked = root.join("a/b");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("lock");
    let denied = list_dir(&locked, &options);
    let total = subtree_total(root, &options);
    let by_path = records(root, &options);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");

    let denied = denied.expect("denied listing");
    assert!(denied.denied);
    assert_eq!(denied.direct_bytes, 0);
    assert!(matches!(
        total,
        Err(ManualSizeError::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied
    ));
    assert!(by_path[&locked].denied);
    assert!(!by_path.contains_key(&root.join("a/b/c")));
}

#[test]
fn concurrent_jobs_never_exceed_the_process_helper_cap() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path().to_path_buf();
    seed_tree(&root);
    let expected = previous_total(&root);

    // Four jobs want 3 helpers each (12 > 8). Each parks at its root until
    // released, so every job holds its helpers at the same moment.
    let jobs = 4;
    let entered = Arc::new(Barrier::new(jobs + 1));
    let release = Arc::new(Barrier::new(jobs + 1));
    let handles: Vec<_> = (0..jobs)
        .map(|_| {
            let (root, entered, release) =
                (root.clone(), Arc::clone(&entered), Arc::clone(&release));
            thread::spawn(move || {
                let mut options = options();
                let parked_root = root.clone();
                options.hooks.before_dir_open = Some(Arc::new(move |dir: &Path| {
                    if dir == parked_root {
                        entered.wait();
                        release.wait();
                    }
                }));
                subtree_total(&root, &options).expect("total")
            })
        })
        .collect();

    entered.wait();
    assert_eq!(helper_threads_for_tests().0, MAX_HELPER_THREADS);
    // With the budget exhausted a job still completes on its own thread.
    assert_eq!(
        subtree_total(&root, &options()).expect("unhelped"),
        expected
    );
    release.wait();

    for handle in handles {
        assert_eq!(handle.join().expect("job"), expected);
    }
    assert_eq!(helper_threads_for_tests(), (0, MAX_HELPER_THREADS));
}

#[test]
fn a_panicking_record_callback_aborts_the_walk_instead_of_hanging() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    seed_tree(root);

    let outcome = std::panic::catch_unwind(|| {
        subtree_records(root, &options(), |_record| panic!("callback failure"))
    });
    assert!(outcome.is_err());
    assert_eq!(helper_threads_for_tests().0, 0);
}
