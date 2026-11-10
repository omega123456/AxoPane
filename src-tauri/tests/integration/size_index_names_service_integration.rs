use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::size::index::names::file::{spill_path, MappedNames, NamesHeader};
use file_explorer_lib::size::index::names::{
    FileNameStore, NamesCounters, NamesScratch, PairExpectation, OVERLAY_THRESHOLD_FOLDERS,
};
use file_explorer_lib::size::index::persist::{self, temp_path, ChangeFlags};
use file_explorer_lib::size::index::platform::{IndexPlatform, PowerSignal, RawEvent};
use file_explorer_lib::size::index::{names_path_beside, IndexRead};
use file_explorer_lib::size::{IndexState, PaneSubscriptions};

use crate::size_index_service_integration::{wait_until, Harness};

const POLL_INTERVAL: Duration = Duration::from_millis(2);
const POLL_LIMIT: Duration = Duration::from_secs(1);
const NAMES_HEADER_BYTES: u64 = 48;
const FIRST_SLOT: u32 = 0;
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

fn snapshot_token(index: &Harness) -> u64 {
    persist::load(&index.snapshot)
        .expect("snapshot")
        .meta
        .pair_token
}

fn names_header(index: &Harness) -> NamesHeader {
    MappedNames::open(&names_file(index))
        .expect("names file")
        .header()
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
    index.start();
    index.wait_state(IndexState::UpToDate);

    assert!(names_file(&index).exists());
    assert!(!spill_path(&names_file(&index)).exists());
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
    assert_eq!(names_header(&index).pair_token, snapshot_token(&index));
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
    index.start();
    index.wait_state(IndexState::UpToDate);
    let written = fs::read(names_file(&index)).unwrap();
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

    assert_eq!(fs::read(names_file(&index)).unwrap(), written, "no rewrite");
    let after = counters(&index);
    assert_eq!(after.blocks_reencoded, before.blocks_reencoded);
    assert_eq!(after.blocks_copied, before.blocks_copied);
}

#[test]
fn a_reload_restores_identical_names_without_rescanning() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    index.start();
    index.wait_state(IndexState::UpToDate);
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
fn a_corrupt_or_mismatched_pair_triggers_exactly_one_full_rescan() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let expected = all_names(&index);

    corrupt(&names_file(&index), 0);
    assert_eq!(
        relaunch_counting_rescans(&mut index),
        1,
        "corrupt names file"
    );
    assert_eq!(index.platform.stream_starts().last(), Some(&None));
    assert_eq!(all_names(&index), expected);

    corrupt(&index.snapshot, 0);
    assert_eq!(relaunch_counting_rescans(&mut index), 1, "corrupt snapshot");
    assert_eq!(all_names(&index), expected);

    let older_names = fs::read(names_file(&index)).unwrap();
    fs::create_dir(index.path("s/extra")).unwrap();
    index.event("s", ChangeFlags::default());
    index.service().flush_index();
    assert_ne!(fs::read(names_file(&index)).unwrap(), older_names);
    index.stop();
    fs::write(names_file(&index), &older_names).unwrap();
    assert_eq!(
        relaunch_counting_rescans(&mut index),
        1,
        "mismatched tokens"
    );
    assert_eq!(names_header(&index).pair_token, snapshot_token(&index));
}

#[test]
fn a_block_that_fails_to_decode_disables_names_and_rescans_once() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.stop();
    corrupt(&names_file(&index), NAMES_HEADER_BYTES);

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
    assert!(names_available(&index), "the rescan rebuilt the pair");
    assert_eq!(indexing_runs(&index), 1);
    assert_eq!(indexed_names(&index, "a"), files_on_disk(&index.path("a")));
}

#[test]
fn a_pair_save_that_needs_a_corrupt_block_rescans_once() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.stop();
    corrupt(&names_file(&index), NAMES_HEADER_BYTES);
    relaunch(&mut index);
    finish_replay(&index);
    index.wait_state(IndexState::UpToDate);

    fs::write(index.path("a/more.txt"), b"m").unwrap();
    index.event("a", ChangeFlags::default());
    index.service().flush_index();
    wait_until("rescan", || {
        indexing_runs(&index) == 1 && index.state() == IndexState::UpToDate
    });
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin", "more.txt"]));
    assert!(names_available(&index));
}

#[test]
fn size_only_changes_keep_the_pair_token_and_structural_changes_save_the_pair() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let token = snapshot_token(&index);
    let names_before = fs::read(names_file(&index)).unwrap();

    fs::write(index.path("a/a.bin"), vec![0; 30]).unwrap();
    index.event("a", ChangeFlags::default());
    blur(&index);
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.tree.total(&index.path("a")).expect("a").bytes, 130);
    assert_eq!(saved.meta.pair_token, token, "folder snapshot only");
    assert_eq!(fs::read(names_file(&index)).unwrap(), names_before);

    fs::create_dir(index.path("a/new")).unwrap();
    index.event("a", ChangeFlags::default());
    blur(&index);
    let paired = snapshot_token(&index);
    assert_ne!(paired, token, "a structural change saves the pair");
    assert_eq!(names_header(&index).pair_token, paired);
}

#[test]
fn blur_saves_the_pair_once_the_overlay_passes_its_threshold() {
    let mut index = Harness::new();
    let many = index.path("many");
    let folders: Vec<PathBuf> = (0..=OVERLAY_THRESHOLD_FOLDERS)
        .map(|number| many.join(format!("d{number}")))
        .collect();
    for folder in &folders {
        fs::create_dir_all(folder).unwrap();
    }
    index.start();
    index.wait_state(IndexState::UpToDate);
    let token = snapshot_token(&index);

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
    blur(&index);
    let paired = snapshot_token(&index);
    assert_ne!(paired, token, "past the overlay threshold");
    assert_eq!(names_header(&index).pair_token, paired);
}

#[test]
fn a_crash_with_names_only_in_the_overlay_replays_from_the_names_event_id() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    index.start();
    index.wait_state(IndexState::UpToDate);

    fs::write(index.path("a/fresh.txt"), b"f").unwrap();
    let change = RawEvent {
        path: index.path("a"),
        id: 600,
        flags: ChangeFlags::default(),
        history_done: false,
    };
    assert!(index.platform.emit(vec![change.clone()]));
    blur(&index);
    assert_eq!(persist::load(&index.snapshot).unwrap().meta.event_id, 600);
    assert_eq!(names_header(&index).event_id, 500);

    index.platform.set_event_id(600);
    relaunch(&mut index);
    assert_eq!(index.platform.stream_starts().last(), Some(&Some(500)));
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin"]));
    let done = RawEvent {
        path: index.path(""),
        history_done: true,
        ..change.clone()
    };
    assert!(index.platform.emit(vec![change, done]));
    index.wait_state(IndexState::UpToDate);
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin", "fresh.txt"]));
}

fn park_the_first_scan_after_a(index: &mut Harness) {
    index.blocker.block(&index.path("a"));
    index.start();
    wait_until("walk reaches a", || index.opened("a"));
    assert!(spill_path(&names_file(index)).exists());
    assert!(!names_file(index).exists());
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
fn the_first_scan_spills_names_and_writes_the_final_pair_one_block_at_a_time() {
    let mut index = Harness::new();
    park_the_first_scan_after_a(&mut index);
    assert!(read(&index, |view| view.names.spill_len()).is_some_and(|len| len > 0));
    assert!(read(&index, |view| view.names.is_spilling()));
    assert!(!read(&index, |view| view.names.has_names_file()));
    assert_eq!(counters(&index).peak_open_output_blocks, 0);

    resume_power(&index);
    assert!(names_file(&index).exists());
    assert!(!spill_path(&names_file(&index)).exists());
    assert_eq!(counters(&index).peak_open_output_blocks, 1);
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin"]));
}

#[test]
fn blur_during_the_first_scan_saves_a_pair_and_keeps_spilling() {
    let mut index = Harness::new();
    park_the_first_scan_after_a(&mut index);
    index.service().window_focus_changed(false);
    poll_until("mid-scan pair", || names_file(&index).exists());
    poll_until("spill restarted", || {
        read(&index, |view| view.names.spill_len()) == Some(0)
    });
    assert_eq!(indexed_names(&index, "a"), owned(&["a.bin"]));

    resume_power(&index);
    assert!(!spill_path(&names_file(&index)).exists());
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
    index.start();
    index.wait_state(IndexState::UpToDate);
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
fn pair_save_failures_never_leave_a_half_written_pair() {
    let names_temp_blocked = scan_with_obstruction(|index| temp_path(&names_file(index)));
    assert!(!names_file(&names_temp_blocked).exists());
    assert!(!names_temp_blocked.snapshot.exists());

    let snapshot_temp_blocked = scan_with_obstruction(|index| temp_path(&index.snapshot));
    assert!(!names_file(&snapshot_temp_blocked).exists());
    assert!(!temp_path(&names_file(&snapshot_temp_blocked)).exists());
    assert!(!snapshot_temp_blocked.snapshot.is_file());

    let names_blocked = scan_with_obstruction(names_file);
    assert!(!names_blocked.snapshot.exists());
    assert!(!temp_path(&names_blocked.snapshot).exists());
    assert!(names_blocked.snapshot.parent().unwrap().exists());

    let snapshot_blocked = scan_with_obstruction(|index| index.snapshot.clone());
    assert!(names_file(&snapshot_blocked).is_file());
    assert!(snapshot_blocked.snapshot.is_dir());
    assert!(!temp_path(&snapshot_blocked.snapshot).exists());
}

fn names_saved_on_disk(index: &Harness) -> Vec<String> {
    let header = names_header(index);
    let mut store = FileNameStore::new(names_file(index));
    let expected = PairExpectation {
        pair_token: header.pair_token,
        first_slot: FIRST_SLOT,
        folder_count: header.folder_count,
    };
    store.open_pair(expected).expect("saved pair");
    let mut scratch = NamesScratch::default();
    let mut saved = Vec::new();
    for block_index in 0..store.block_count() {
        let block = store.read_block(block_index, &mut scratch).expect("block");
        for folder in block.folders() {
            saved.extend(sorted_strings(folder.names()));
        }
    }
    saved.sort();
    saved
}

#[test]
fn an_exclusion_removes_its_names_from_disk_without_waiting_for_blur() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let token = snapshot_token(&index);
    assert!(names_saved_on_disk(&index).contains(&"b.bin".to_string()));

    index
        .service()
        .add_index_exclusions(&[index.path("a")])
        .expect("index attached");

    poll_until("pair saved", || snapshot_token(&index) != token);
    assert_eq!(names_saved_on_disk(&index), ["h.bin", "s.bin", "top.bin"]);
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.meta.exclusions, [index.path("a")]);
    assert!(saved.tree.lookup(&index.path("a/b")).is_none());
}
