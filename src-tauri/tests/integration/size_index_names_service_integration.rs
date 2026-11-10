use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::size::index::names::file::spill_path;
use file_explorer_lib::size::index::names::{
    FileNameStore, NamesCounters, NamesScratch, OVERLAY_THRESHOLD_FOLDERS,
};
use file_explorer_lib::size::index::persist::{self, temp_path, ChangeFlags, SegmentRef, Snapshot};
use file_explorer_lib::size::index::platform::{IndexPlatform, PowerSignal, RawEvent};
use file_explorer_lib::size::index::segment::SegmentPaths;
use file_explorer_lib::size::index::{names_path_beside, IndexRead};
use file_explorer_lib::size::{IndexState, PaneSubscriptions};

use crate::size_index_service_integration::{wait_until, Harness};

const POLL_INTERVAL: Duration = Duration::from_millis(2);
const POLL_LIMIT: Duration = Duration::from_secs(1);
const NAMES_HEADER_BYTES: u64 = 48;
const BARRIER_PANE: &str = "names-barrier";
const BARRIER_FOLDER: &str = "s";

fn poll_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + POLL_LIMIT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(POLL_INTERVAL);
    }
}

fn names_file(index: &Harness) -> PathBuf {
    names_path_beside(&index.snapshot)
}

fn read<R>(index: &Harness, read: impl FnOnce(IndexRead<'_>) -> R) -> R {
    index
        .service()
        .folder_size_index()
        .expect("index attached")
        .read_index(read)
        .expect("readable")
}

fn sorted_strings<'name>(names: impl Iterator<Item = &'name [u8]>) -> Vec<String> {
    let mut listed: Vec<String> = names
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    listed.sort();
    listed
}

fn indexed_names(index: &Harness, relative: &str) -> Option<Vec<String>> {
    let path = index.path(relative);
    read(index, |view| {
        let slot = view.tree.lookup(&path)?;
        let names = view.names.names_of(slot).expect("names readable")?;
        Some(sorted_strings(names.names()))
    })
}

fn owned(names: &[&str]) -> Option<Vec<String>> {
    Some(names.iter().map(|name| name.to_string()).collect())
}

fn files_on_disk(dir: &Path) -> Option<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("fixture folder")
        .map(|entry| entry.expect("entry"))
        .filter(|entry| entry.file_type().expect("type").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Some(names)
}

fn names_available(index: &Harness) -> bool {
    index
        .service()
        .folder_size_index_status()
        .file_names_available
}

fn counters(index: &Harness) -> NamesCounters {
    read(index, |view| view.names.counters())
}

fn barrier_subscription(folders: Vec<String>) -> PaneSubscriptions {
    PaneSubscriptions {
        pane_id: BARRIER_PANE.into(),
        current_path: None,
        folders,
    }
}

fn barrier(index: &Harness) {
    let before = index.live_for(BARRIER_FOLDER).len();
    let subscribed = barrier_subscription(vec![index.key(BARRIER_FOLDER)]);
    index.service().set_subscriptions(subscribed);
    wait_until("index thread barrier", || {
        index.live_for(BARRIER_FOLDER).len() > before
    });
    index
        .service()
        .set_subscriptions(barrier_subscription(Vec::new()));
}

fn blur(index: &Harness) {
    index.service().window_focus_changed(false);
    barrier(index);
}

fn saved(index: &Harness) -> Snapshot {
    persist::load(&index.snapshot).expect("saved manifest")
}

fn is_saved(index: &Harness) -> bool {
    persist::load(&index.snapshot).is_some()
}

fn base_segment(index: &Harness) -> SegmentRef {
    let segments = saved(index).segments;
    assert_eq!(segments.len(), 1, "one base segment");
    segments[0]
}

fn paths_of(index: &Harness, segment: SegmentRef) -> SegmentPaths {
    let directory = index.snapshot.parent().expect("cache directory");
    SegmentPaths::numbered(directory, segment.sequence)
}

fn base_paths(index: &Harness) -> SegmentPaths {
    paths_of(index, base_segment(index))
}

fn base_names_file(index: &Harness) -> PathBuf {
    base_paths(index).names
}

fn wait_for_the_base(index: &Harness) {
    poll_until("the base segment", || is_saved(index));
}

fn scan_and_save(index: &mut Harness) {
    index.start();
    index.wait_state(IndexState::UpToDate);
    wait_for_the_base(index);
}

fn wait_for_a_new_base(index: &Harness, previous: SegmentRef) {
    poll_until("a new base segment", || {
        persist::load(&index.snapshot).is_some_and(|snapshot| snapshot.segments != [previous])
    });
}

fn wait_for_the_spill_to_go(index: &Harness) {
    poll_until("the spill is folded into the base", || {
        !spill_path(&names_file(index)).exists()
    });
}

fn indexing_runs(index: &Harness) -> usize {
    let statuses = index.statuses.lock().unwrap();
    let previous =
        std::iter::once(IndexState::NotReady).chain(statuses.iter().map(|status| status.state));
    statuses
        .iter()
        .zip(previous)
        .filter(|(status, before)| {
            status.state == IndexState::Indexing && *before != IndexState::Indexing
        })
        .count()
}

fn corrupt(path: &Path, offset: u64) {
    let mut file = OpenOptions::new().write(true).open(path).expect("open");
    file.seek(SeekFrom::Start(offset)).expect("seek");
    file.write_all(&[0xEE; 4]).expect("corrupt");
}

fn finish_replay(index: &Harness) {
    let done = RawEvent {
        path: index.path(""),
        id: index.platform.current_event_id(),
        flags: ChangeFlags::default(),
        history_done: true,
    };
    assert!(index.platform.emit(vec![done]), "stream running");
}

fn relaunch(index: &mut Harness) {
    index.stop();
    index.statuses.lock().unwrap().clear();
    index.opened.lock().unwrap().clear();
    index.start();
}

fn relaunch_counting_rescans(index: &mut Harness) -> usize {
    relaunch(index);
    wait_until("rescan finished", || {
        indexing_runs(index) > 0 && index.state() == IndexState::UpToDate
    });
    indexing_runs(index)
}

const FOLDERS: [&str; 5] = ["", "a", "a/b", "s", "home"];

fn all_names(index: &Harness) -> Vec<Option<Vec<String>>> {
    FOLDERS
        .iter()
        .map(|relative| indexed_names(index, relative))
        .collect()
}

#[test]
fn a_full_scan_stores_every_folders_file_names_including_hidden_ones() {
    let mut index = Harness::new();
    fs::write(index.path("a/.hidden"), b"h").unwrap();
    symlink(index.path("top.bin"), index.path("a/link")).unwrap();
    scan_and_save(&mut index);

    assert!(base_names_file(&index).exists());
    assert!(!names_file(&index).exists(), "no separate names file");
    wait_for_the_spill_to_go(&index);
    assert!(names_available(&index));
    for relative in FOLDERS {
        assert_eq!(
            indexed_names(&index, relative),
            files_on_disk(&index.path(relative)),
            "{relative:?}"
        );
    }
    assert_eq!(indexed_names(&index, "a"), owned(&[".hidden", "a.bin"]));
    assert_eq!(
        indexed_names(&index, "home/Documents"),
        owned(&[]),
        "a protected folder is never read"
    );
    assert!(saved(&index).meta.has_file_names);
    let first = index
        .statuses
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("status");
    assert!(!first.file_names_available, "nothing exists before boot");
}

#[test]
fn re_reads_update_names_without_a_rewrite_and_prunes_hide_them() {
    let mut index = Harness::new();
    fs::write(index.path("a/draft.txt"), b"d").unwrap();
    scan_and_save(&mut index);
    let base = base_segment(&index);
    let written = fs::read(base_names_file(&index)).unwrap();
    let before = counters(&index);

    fs::write(index.path("a/new.txt"), b"n").unwrap();
    fs::remove_file(index.path("a/a.bin")).unwrap();
    fs::rename(index.path("a/draft.txt"), index.path("a/final.txt")).unwrap();
    index.event("a", ChangeFlags::default());
    barrier(&index);
    assert_eq!(indexed_names(&index, "a"), owned(&["final.txt", "new.txt"]));

    let pruned_slot = read(&index, |view| view.tree.lookup(&index.path("a/b"))).expect("b");
    assert_eq!(indexed_names(&index, "a/b"), owned(&["b.bin"]));
    fs::remove_dir_all(index.path("a/b")).unwrap();
    index.event("a", ChangeFlags::default());
    barrier(&index);
    assert_eq!(
        read(&index, |view| view.tree.lookup(&index.path("a/b"))),
        None
    );
    let hidden = read(&index, |view| {
        view.names.names_of(pruned_slot).expect("readable")
    });
    assert_eq!(hidden.map(|names| names.is_empty()), Some(true));

    blur(&index);
    assert_eq!(base_segment(&index), base, "no new segment");
    assert_eq!(fs::read(base_names_file(&index)).unwrap(), written);
    assert_eq!(counters(&index).blocks_reencoded, before.blocks_reencoded);
}

#[test]
fn a_reload_restores_identical_names_without_rescanning() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    scan_and_save(&mut index);
    let before = all_names(&index);

    relaunch(&mut index);
    assert_eq!(index.platform.stream_starts().last(), Some(&Some(500)));
    assert!(names_available(&index));
    assert_eq!(all_names(&index), before);
    finish_replay(&index);
    index.wait_state(IndexState::UpToDate);
    assert!(!index.opened(""), "no rescan");
    assert_eq!(indexing_runs(&index), 0);
    assert_eq!(all_names(&index), before);
}

#[test]
fn a_corrupt_or_mismatched_segment_triggers_exactly_one_full_rescan() {
    let mut index = Harness::new();
    scan_and_save(&mut index);
    let expected = all_names(&index);
    index.stop();

    corrupt(&base_names_file(&index), 0);
    assert_eq!(
        relaunch_counting_rescans(&mut index),
        1,
        "corrupt names file"
    );
    assert_eq!(index.platform.stream_starts().last(), Some(&None));
    assert_eq!(all_names(&index), expected);
    wait_for_the_base(&index);
    index.stop();

    corrupt(&index.snapshot, 0);
    assert_eq!(relaunch_counting_rescans(&mut index), 1, "corrupt manifest");
    assert_eq!(all_names(&index), expected);
    wait_for_the_base(&index);

    let older = base_segment(&index);
    let older_tree = fs::read(paths_of(&index, older).tree).unwrap();
    let older_names = fs::read(paths_of(&index, older).names).unwrap();
    index
        .service()
        .rebuild_folder_size_index()
        .expect("index attached");
    wait_for_a_new_base(&index, older);
    index.stop();
    let newer_paths = base_paths(&index);
    fs::write(&newer_paths.tree, &older_tree).unwrap();
    fs::write(&newer_paths.names, &older_names).unwrap();
    assert_eq!(
        relaunch_counting_rescans(&mut index),
        1,
        "mismatched tokens"
    );
    assert_eq!(all_names(&index), expected);
}

#[test]
fn a_block_that_fails_to_decode_disables_names_and_rescans_once() {
    let mut index = Harness::new();
    scan_and_save(&mut index);
    index.stop();
    corrupt(&base_names_file(&index), NAMES_HEADER_BYTES);

    relaunch(&mut index);
    finish_replay(&index);
    index.wait_state(IndexState::UpToDate);
    assert!(names_available(&index), "the header is intact");
    index.blocker.block(&index.path(""));
    let failed = read(&index, |view| {
        let slot = view.tree.lookup(&index.path("a")).expect("a");
        view.names.names_of(slot).is_err()
    });
    assert!(failed);
    wait_until("names unavailable while rescanning", || {
        let statuses = index.statuses.lock().unwrap();
        statuses
            .iter()
            .any(|status| status.state == IndexState::Indexing && !status.file_names_available)
    });
    assert!(!names_available(&index));

    index.blocker.release(&index.path(""));
    index.wait_state(IndexState::UpToDate);
    wait_until("the rescan rebuilt the names", || names_available(&index));
    assert_eq!(indexing_runs(&index), 1);
    assert_eq!(indexed_names(&index, "a"), files_on_disk(&index.path("a")));
}

#[test]
fn a_compaction_that_needs_a_corrupt_block_rescans_once() {
    let mut index = Harness::new();
    scan_and_save(&mut index);
    index.stop();
    corrupt(&base_names_file(&index), NAMES_HEADER_BYTES);
    relaunch(&mut index);
    finish_replay(&index);
    index.wait_state(IndexState::UpToDate);

    fs::write(index.path("a/more.txt"), b"m").unwrap();
    index.event("a", ChangeFlags::default());
    index
        .service()
        .add_index_exclusions(&[index.path("s")])
        .expect("index attached");
    wait_until("rescan", || {
        indexing_runs(&index) == 1 && index.state() == IndexState::UpToDate
    });
    wait_until("names rebuilt", || names_available(&index));
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin", "more.txt"]));
}

#[test]
fn blur_saves_changes_in_the_manifest_and_leaves_the_segment_untouched() {
    let mut index = Harness::new();
    scan_and_save(&mut index);
    let base = base_segment(&index);
    let names_before = fs::read(base_names_file(&index)).unwrap();

    fs::write(index.path("a/a.bin"), vec![0; 30]).unwrap();
    fs::write(index.path("a/fresh.txt"), b"").unwrap();
    index.event("a", ChangeFlags::default());
    blur(&index);
    let resized = saved(&index);
    assert_eq!(resized.tree.total(&index.path("a")).expect("a").bytes, 130);
    assert_eq!(resized.segments, [base]);

    fs::create_dir(index.path("a/new")).unwrap();
    index.event("a", ChangeFlags::default());
    blur(&index);
    let grown = saved(&index);
    assert!(grown.tree.lookup(&index.path("a/new")).is_some());
    assert_eq!(grown.segments, [base], "the overlay holds the new folder");
    assert_eq!(fs::read(base_names_file(&index)).unwrap(), names_before);
    assert_eq!(
        names_saved_on_disk(&index, "a"),
        ["a.bin", "fresh.txt"],
        "overlay names are saved in the manifest"
    );
}

fn create_more_folders_than_the_overlay_holds(index: &Harness) -> Vec<PathBuf> {
    let many = index.path("many");
    let folders: Vec<PathBuf> = (0..=OVERLAY_THRESHOLD_FOLDERS)
        .map(|number| many.join(format!("d{number}")))
        .collect();
    for folder in &folders {
        fs::create_dir_all(folder).unwrap();
    }
    folders
}

#[test]
fn an_overlay_past_its_threshold_is_compacted_into_a_new_base() {
    let mut index = Harness::new();
    let folders = create_more_folders_than_the_overlay_holds(&index);
    scan_and_save(&mut index);
    let base = base_segment(&index);

    let events = folders
        .iter()
        .map(|folder| RawEvent {
            path: folder.clone(),
            id: 300,
            flags: ChangeFlags::default(),
            history_done: false,
        })
        .collect();
    assert!(index.platform.emit(events));
    wait_for_a_new_base(&index, base);
    let overlay = read(&index, |view| view.names.overlay_stats());
    assert_eq!(overlay.folders, 0, "the new base absorbed the overlay");
    let old_base = paths_of(&index, base);
    poll_until("the old base is swept", || !old_base.tree.exists());
    assert!(!old_base.names.exists());
}

#[test]
fn a_relaunch_resumes_from_names_saved_in_the_manifest_overlay() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    scan_and_save(&mut index);

    fs::write(index.path("a/fresh.txt"), b"f").unwrap();
    let change = RawEvent {
        path: index.path("a"),
        id: 600,
        flags: ChangeFlags::default(),
        history_done: false,
    };
    assert!(index.platform.emit(vec![change]));
    blur(&index);
    assert_eq!(saved(&index).meta.event_id, 600);

    index.platform.set_event_id(600);
    relaunch(&mut index);
    assert_eq!(index.platform.stream_starts().last(), Some(&Some(600)));
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin", "fresh.txt"]));
    finish_replay(&index);
    index.wait_state(IndexState::UpToDate);
    assert_eq!(indexing_runs(&index), 0);
}

fn park_the_first_scan_after_a(index: &mut Harness) {
    index.blocker.block(&index.path("a"));
    index.start();
    wait_until("walk reaches a", || index.opened("a"));
    assert!(spill_path(&names_file(index)).exists());
    assert!(!index.snapshot.exists());
    wait_until("a spill counts as names", || names_available(index));
    index.platform.signal(PowerSignal::Conditions {
        low_power: true,
        thermal_pressure: false,
    });
    wait_until("paused", || index.state() == IndexState::Paused);
    index.blocker.release(&index.path("a"));
    wait_until("walk parked", || index.parked.load(Ordering::SeqCst) > 0);
    poll_until("spilled names", || {
        indexed_names(index, "a") == owned(&["a.bin"])
    });
}

fn resume_power(index: &Harness) {
    index.platform.signal(PowerSignal::Conditions {
        low_power: false,
        thermal_pressure: false,
    });
    index.wait_state(IndexState::UpToDate);
}

#[test]
fn the_first_scan_spills_names_and_writes_the_base_one_block_at_a_time() {
    let mut index = Harness::new();
    park_the_first_scan_after_a(&mut index);
    assert!(read(&index, |view| view.names.spill_len()).is_some_and(|len| len > 0));
    assert!(read(&index, |view| view.names.is_spilling()));
    assert!(!read(&index, |view| view.names.has_saved_names()));
    assert_eq!(counters(&index).peak_open_output_blocks, 0);

    resume_power(&index);
    wait_for_the_base(&index);
    assert!(base_names_file(&index).exists());
    wait_for_the_spill_to_go(&index);
    assert_eq!(counters(&index).peak_open_output_blocks, 1);
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin"]));
}

#[test]
fn blur_during_the_first_scan_saves_a_base_and_keeps_spilling() {
    let mut index = Harness::new();
    park_the_first_scan_after_a(&mut index);
    index.service().window_focus_changed(false);
    wait_for_the_base(&index);
    poll_until("spill restarted", || {
        read(&index, |view| view.names.spill_len()) == Some(0)
    });
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin"]));
    let mid_scan = base_segment(&index);

    resume_power(&index);
    wait_for_a_new_base(&index, mid_scan);
    wait_for_the_spill_to_go(&index);
    for relative in FOLDERS {
        assert_eq!(
            indexed_names(&index, relative),
            files_on_disk(&index.path(relative))
        );
    }
}

#[test]
fn a_stale_spill_found_at_launch_is_deleted() {
    let mut index = Harness::new();
    scan_and_save(&mut index);
    index.stop();
    let spill = spill_path(&names_file(&index));
    fs::write(&spill, b"left over by a crash").unwrap();
    index.start();
    assert!(!spill.exists());
    finish_replay(&index);
    index.wait_state(IndexState::UpToDate);
    assert!(!spill.exists(), "no full scan, so no new spill");
}

fn obstruct(path: &Path) {
    fs::create_dir_all(path.join("occupied")).unwrap();
}

fn scan_with_obstruction(target: impl Fn(&Harness) -> PathBuf) -> Harness {
    let mut index = Harness::new();
    obstruct(&target(&index));
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.service().flush_index();
    index
}

#[test]
fn manifest_save_failures_never_leave_a_half_written_manifest() {
    let temp_blocked = scan_with_obstruction(|index| temp_path(&index.snapshot));
    assert!(!temp_blocked.snapshot.exists());
    assert_eq!(indexed_names(&temp_blocked, "a"), owned(&["a.bin"]));

    let manifest_blocked = scan_with_obstruction(|index| index.snapshot.clone());
    assert!(manifest_blocked.snapshot.is_dir());
    assert!(!temp_path(&manifest_blocked.snapshot).exists());
    assert_eq!(indexed_names(&manifest_blocked, "a"), owned(&["a.bin"]));
}

fn names_saved_on_disk(index: &Harness, relative: &str) -> Vec<String> {
    let Snapshot { tree, names, .. } = saved(index);
    let mut store = FileNameStore::new(names_file(index));
    store.restore(names);
    let slot = tree.lookup(&index.path(relative)).expect("saved folder");
    let saved_names = store.names_of(slot).expect("readable").expect("names");
    sorted_strings(saved_names.names())
}

fn every_name_in_saved_blocks(index: &Harness) -> Vec<String> {
    let mut store = FileNameStore::new(names_file(index));
    store.restore(saved(index).names);
    let mut scratch = NamesScratch::default();
    let mut names = Vec::new();
    for block_index in 0..store.block_count() {
        let block = store.read_block(block_index, &mut scratch).expect("block");
        for folder in block.folders() {
            names.extend(sorted_strings(folder.names()));
        }
    }
    names.sort();
    names
}

#[test]
fn an_exclusion_removes_its_names_from_disk_without_waiting_for_blur() {
    let mut index = Harness::new();
    scan_and_save(&mut index);
    let base = base_segment(&index);
    assert!(every_name_in_saved_blocks(&index).contains(&"b.bin".to_string()));

    index
        .service()
        .add_index_exclusions(&[index.path("a")])
        .expect("index attached");

    wait_for_a_new_base(&index, base);
    assert_eq!(
        every_name_in_saved_blocks(&index),
        ["h.bin", "s.bin", "top.bin"]
    );
    let saved = saved(&index);
    assert_eq!(saved.meta.exclusions, [index.path("a")]);
    assert!(saved.tree.lookup(&index.path("a/b")).is_none());
}

#[test]
fn a_segment_left_by_a_crash_before_its_manifest_is_swept_at_launch() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    scan_and_save(&mut index);
    index.stop();
    let base = base_segment(&index);
    let never_committed = SegmentRef {
        sequence: base.sequence + 1,
        ..base
    };
    let orphan = paths_of(&index, never_committed);
    fs::copy(paths_of(&index, base).tree, &orphan.tree).unwrap();
    fs::copy(paths_of(&index, base).names, &orphan.names).unwrap();

    relaunch(&mut index);
    assert_eq!(
        index.platform.stream_starts().last(),
        Some(&Some(500)),
        "the manifest still resumes"
    );
    assert!(!orphan.tree.exists());
    assert!(!orphan.names.exists());
    assert_eq!(base_segment(&index), base);

    index
        .service()
        .add_index_exclusions(&[index.path("s")])
        .expect("index attached");
    wait_for_a_new_base(&index, base);
    assert!(
        base_segment(&index).sequence > never_committed.sequence,
        "a sequence seen on disk is never reused"
    );
}

#[test]
fn events_arriving_during_a_compaction_are_applied_after_it() {
    let mut index = Harness::new();
    create_more_folders_than_the_overlay_holds(&index);
    scan_and_save(&mut index);
    let base = base_segment(&index);

    index
        .service()
        .add_index_exclusions(&[index.path("s")])
        .expect("index attached");
    fs::write(index.path("a/late.txt"), b"l").unwrap();
    index.event("a", ChangeFlags::default());

    wait_for_a_new_base(&index, base);
    poll_until("the late event is applied", || {
        indexed_names(&index, "a") == owned(&["a.bin", "late.txt"])
    });
    index.wait_state(IndexState::UpToDate);
    assert!(!every_name_in_saved_blocks(&index).contains(&"s.bin".to_string()));
}

fn saved_base_is_purged_of(index: &Harness, file_name: &str) -> bool {
    let Some(snapshot) = persist::load(&index.snapshot) else {
        return false;
    };
    let holds_the_name = every_name_in_saved_blocks(index).contains(&file_name.to_string());
    !snapshot.meta.names_purge_due && !holds_the_name
}

#[test]
fn a_quit_during_a_compaction_saves_the_manifest_and_purges_at_the_next_launch() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    create_more_folders_than_the_overlay_holds(&index);
    scan_and_save(&mut index);

    index
        .service()
        .add_index_exclusions(&[index.path("a")])
        .expect("index attached");
    let asked = Instant::now();
    index.service().flush_index();
    assert!(asked.elapsed() < POLL_LIMIT, "the exit save never waits");
    assert_eq!(saved(&index).meta.exclusions, [index.path("a")]);

    relaunch(&mut index);
    assert_eq!(index.platform.stream_starts().last(), Some(&Some(500)));
    finish_replay(&index);
    poll_until("the excluded names leave the disk", || {
        saved_base_is_purged_of(&index, "b.bin")
    });
    index.wait_state(IndexState::UpToDate);
    assert_eq!(indexing_runs(&index), 0, "no rescan");
}
