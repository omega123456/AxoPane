use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use file_explorer_lib::size::index::names::file::{NamesHeader, NAMES_FILE};
use file_explorer_lib::size::index::names::writer::{
    is_past, write_names, PairStamp, RewriteError, RewriteStats,
};
use file_explorer_lib::size::index::names::{
    names_in, FileNameStore, FolderNames, InstallError, NamesCounters, NamesScratch,
    NamesUnreadable, OverlayStats, PackedNames, PairExpectation, OVERLAY_THRESHOLD_BYTES,
    OVERLAY_THRESHOLD_FOLDERS,
};
use file_explorer_lib::size::index::persist::temp_path;
use tempfile::{tempdir, TempDir};

const SMALL_BLOCK_TARGET: usize = 64;
const HEADER_BYTES: u64 = 48;
const FIRST_SLOT: u32 = 0;

fn packed(names: &[&str]) -> PackedNames {
    PackedNames::from_names(names.iter().map(|name| name.as_bytes()))
}

fn strings(names: &PackedNames) -> Vec<String> {
    names
        .names()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

/// Every folder gets three distinctive names so each takes a few dozen
/// bytes and a 64-byte block target yields roughly one block per folder.
fn names_for(slot: u32) -> PackedNames {
    packed(&[
        &format!("report-{slot}.pdf"),
        &format!(".hidden-{slot}"),
        &format!("notes-{slot}.md"),
    ])
}

struct Fixture {
    _dir: TempDir,
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("cache").join(NAMES_FILE);
        Self { _dir: dir, path }
    }

    fn store(&self) -> FileNameStore {
        FileNameStore::with_block_target(self.path.clone(), SMALL_BLOCK_TARGET)
    }
}

fn stamp(pair_token: u64) -> PairStamp {
    PairStamp {
        event_id: 900,
        pair_token,
        deadline: None,
    }
}

/// Writes the store's current names in `order` and installs the file.
fn rewrite(store: &mut FileNameStore, order: Vec<u32>, pair_token: u64) -> RewriteStats {
    let written = write_names(store.capture(order), stamp(pair_token)).expect("written");
    let stats = written.stats;
    store.install(written, || Ok(())).expect("installed");
    stats
}

/// A store whose names file holds `slots` (each with `names_for`).
fn written_store(fixture: &Fixture, slots: &[u32]) -> FileNameStore {
    let mut store = fixture.store();
    for &slot in slots {
        store.set_names(slot, names_for(slot));
    }
    rewrite(&mut store, slots.to_vec(), 1);
    store
}

fn current(store: &FileNameStore, slot: u32) -> Option<Vec<String>> {
    store
        .names_of(slot)
        .expect("readable")
        .map(|names| strings(&names))
}

fn block_folders(store: &FileNameStore, index: usize) -> Vec<u32> {
    let mut scratch = NamesScratch::default();
    let block = store.read_block(index, &mut scratch).expect("block");
    block.folders().map(|folder| folder.slot).collect()
}

fn corrupt_byte_at(path: &PathBuf, offset: u64) {
    let mut file = OpenOptions::new().write(true).open(path).expect("open");
    file.seek(SeekFrom::Start(offset)).expect("seek");
    file.write_all(&[0xEE, 0xEE, 0xEE, 0xEE]).expect("corrupt");
}

#[test]
fn packed_names_keep_hidden_and_unicode_names_and_skip_impossible_ones() {
    let mut names = packed(&[".env", "Ünïcödé.txt"]);
    names.push(b"");
    names.push(b"bad\0name");
    names.push(b"ok");
    assert_eq!(strings(&names), [".env", "Ünïcödé.txt", "ok"]);
    assert_eq!(
        names.as_bytes(),
        b".env\0\xc3\x9cn\xc3\xafc\xc3\xb6d\xc3\xa9.txt\0ok\0"
    );
    assert!(!names.is_empty());
    assert!(PackedNames::default().is_empty());
    assert_eq!(names_in(b"").count(), 0);
    assert_eq!(names_in(b"a\0b\0").collect::<Vec<_>>(), [b"a", b"b"]);
    let folder = FolderNames {
        slot: 3,
        packed: names.as_bytes(),
    };
    assert_eq!(folder.names().count(), 3);
}

#[test]
fn sorting_packed_names_orders_them_bytewise_and_keeps_every_name() {
    let mut names = packed(&["b.txt", "Zeta", "a.txt", ".env", "a.txt", "Ünïcödé"]);
    names.sort();
    assert_eq!(
        strings(&names),
        [".env", "Zeta", "a.txt", "a.txt", "b.txt", "Ünïcödé"]
    );

    let mut empty = PackedNames::default();
    empty.sort();
    assert!(empty.is_empty());
}

#[test]
fn a_fresh_store_has_no_names_until_a_file_or_spill_exists() {
    let fixture = Fixture::new();
    let store = FileNameStore::new(fixture.path.clone());
    assert_eq!(store.path(), fixture.path);
    assert!(!store.is_available());
    assert!(!store.has_names_file());
    assert!(!store.is_spilling());
    assert_eq!(store.spill_len(), None);
    assert_eq!(store.names_header(), None);
    assert_eq!(store.block_count(), 0);
    assert_eq!(store.blocks_covering(0..10), 0..0);
    assert!(store.blocks_for_slots(&[1, 2]).is_empty());
    assert_eq!(store.ordinal_of(0), None);
    assert_eq!(store.slot_of(0), None);
    assert_eq!(store.names_of(4), Ok(None));
    assert!(!store.has_unsaved_names());
    assert_eq!(store.counters(), NamesCounters::default());
    let mut scratch = NamesScratch::default();
    assert!(store.read_block(0, &mut scratch).is_err());
    let mut visited = 0;
    store
        .visit_spill(&mut scratch, &mut |_| visited += 1)
        .expect("no spill");
    assert_eq!(visited, 0);
}

#[test]
fn the_overlay_beats_blocks_and_tombstones_hide_pruned_folders() {
    let fixture = Fixture::new();
    let mut store = written_store(&fixture, &[0, 1, 2, 3]);
    assert!(store.is_available());
    assert!(
        !store.has_unsaved_names(),
        "the rewrite cleared the overlay"
    );
    assert_eq!(current(&store, 2), Some(strings(&names_for(2))));

    store.set_names(2, packed(&["fresh.txt"]));
    store.tombstone(3);
    assert_eq!(current(&store, 2), Some(vec!["fresh.txt".to_string()]));
    assert_eq!(current(&store, 3), Some(Vec::new()), "pruned");
    assert!(store.has_unsaved_names());
    assert_eq!(
        store.overlay_stats(),
        OverlayStats {
            folders: 2,
            name_bytes: "fresh.txt\0".len(),
        }
    );
    let shown: Vec<u32> = (0..store.block_count())
        .flat_map(|index| block_folders(&store, index))
        .collect();
    assert_eq!(shown, [0, 1], "blocks skip folders the overlay replaced");
    let mut overlay = Vec::new();
    store.visit_overlay(&mut |folder| overlay.push(folder.slot));
    assert_eq!(overlay, [2], "tombstones carry no names");

    store.set_names(3, packed(&["reused.slot"]));
    assert_eq!(current(&store, 3), Some(vec!["reused.slot".to_string()]));
    store.set_names(3, packed(&["x"]));
    assert_eq!(store.overlay_stats().name_bytes, "fresh.txt\0x\0".len());
}

#[test]
fn a_rewrite_copies_unchanged_blocks_and_re_encodes_only_the_changed_one() {
    let fixture = Fixture::new();
    let slots: Vec<u32> = (0..12).collect();
    let mut store = fixture.store();
    for &slot in &slots {
        store.set_names(slot, names_for(slot));
    }
    let first = rewrite(&mut store, slots.clone(), 1);
    assert_eq!(first.blocks_copied, 0);
    assert_eq!(first.blocks_reencoded as usize, store.block_count());
    assert!(store.block_count() >= 6, "{} blocks", store.block_count());

    let changed = 7;
    let changed_ordinal = store.ordinal_of(changed).expect("ordinal");
    let containing = store.blocks_covering(changed_ordinal..changed_ordinal + 1);
    store.set_names(changed, packed(&["edited.txt"]));
    let before = store.counters();
    let second = rewrite(&mut store, slots.clone(), 2);
    assert_eq!(second.blocks_reencoded, containing.len() as u64);
    assert_eq!(
        second.blocks_copied as usize,
        store.block_count() - containing.len()
    );
    let after = store.counters();
    assert_eq!(
        after.blocks_reencoded - before.blocks_reencoded,
        second.blocks_reencoded
    );
    assert_eq!(
        after.blocks_copied - before.blocks_copied,
        second.blocks_copied
    );
    assert_eq!(after.peak_open_output_blocks, 1);
    for &slot in &slots {
        let expected = match slot {
            7 => vec!["edited.txt".to_string()],
            _ => strings(&names_for(slot)),
        };
        assert_eq!(current(&store, slot), Some(expected), "slot {slot}");
    }
}

#[test]
fn a_new_folder_shifts_later_blocks_which_are_still_copied() {
    let fixture = Fixture::new();
    let slots: Vec<u32> = (0..10).collect();
    let mut store = written_store(&fixture, &slots);
    let blocks_before = store.block_count();

    let new_slot = 40;
    store.set_names(new_slot, names_for(new_slot));
    let mut order = slots.clone();
    order.insert(3, new_slot);
    let stats = rewrite(&mut store, order.clone(), 2);
    assert!(
        stats.blocks_copied as usize >= blocks_before - 2,
        "{stats:?}"
    );
    assert!(stats.blocks_reencoded <= 2, "{stats:?}");
    assert_eq!(store.ordinal_of(new_slot), Some(3));
    assert_eq!(store.slot_of(4), Some(3));
    for &slot in &order {
        assert_eq!(current(&store, slot), Some(strings(&names_for(slot))));
    }
}

#[test]
fn the_translation_table_maps_file_ordinals_to_drifted_slots() {
    let fixture = Fixture::new();
    let order = vec![9, 4, 7, 1];
    let mut store = fixture.store();
    for &slot in &order {
        store.set_names(slot, names_for(slot));
    }
    rewrite(&mut store, order.clone(), 1);
    for (ordinal, &slot) in order.iter().enumerate() {
        assert_eq!(store.ordinal_of(slot), Some(ordinal as u32));
        assert_eq!(store.slot_of(ordinal as u32), Some(slot));
        assert_eq!(current(&store, slot), Some(strings(&names_for(slot))));
    }
    assert_eq!(store.ordinal_of(5), None, "never written");
    assert_eq!(store.slot_of(99), None);
    assert_eq!(current(&store, 5), None);
    let blocks = store.blocks_for_slots(&[1, 9, 5]);
    assert_eq!(blocks.first(), Some(&0));
    assert_eq!(blocks.last(), Some(&(store.block_count() - 1)));
    let mut scratch = NamesScratch::default();
    let last = store
        .read_block(store.block_count() - 1, &mut scratch)
        .expect("block");
    assert_eq!(last.span().end(), order.len() as u32);
    assert_eq!(store.counters().blocks_decompressed, 1 + order.len() as u64);
}

#[test]
fn the_spill_serves_reads_during_a_scan_and_streams_into_one_block_at_a_time() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    store.begin_spill().expect("spill");
    assert!(store.is_available(), "names exist once a spill exists");
    assert!(store.is_spilling());
    assert_eq!(store.spill_len(), Some(0));
    for slot in 0..8 {
        store.spill_names(slot, names_for(slot));
    }
    let grown = store.spill_len().expect("spill");
    assert!(grown > 0);
    assert!(store.spill_path().exists());
    assert_eq!(current(&store, 5), Some(strings(&names_for(5))));

    store.set_names(5, packed(&["newer.txt"]));
    assert_eq!(current(&store, 5), Some(vec!["newer.txt".to_string()]));
    store.tombstone(6);
    assert_eq!(current(&store, 6), Some(Vec::new()));
    store.spill_names(6, packed(&["reused.slot"]));
    assert_eq!(current(&store, 6), Some(vec!["reused.slot".to_string()]));

    let mut scratch = NamesScratch::default();
    let mut spilled = Vec::new();
    store
        .visit_spill(&mut scratch, &mut |folder| spilled.push(folder.slot))
        .expect("spill readable");
    assert_eq!(spilled, [0, 1, 2, 3, 4, 6, 7], "the overlay shadows slot 5");

    let stats = rewrite(&mut store, (0..8).collect(), 1);
    assert_eq!(stats.peak_open_blocks, 1);
    assert!(!store.is_spilling());
    assert!(
        !store.spill_path().exists(),
        "the spill is removed after the pair"
    );
    assert_eq!(current(&store, 6), Some(vec!["reused.slot".to_string()]));
    assert_eq!(current(&store, 5), Some(vec!["newer.txt".to_string()]));
    assert_eq!(store.counters().peak_open_output_blocks, 1);
}

#[test]
fn a_pair_save_during_a_scan_reads_spilled_folders_between_folders_of_one_mapped_block() {
    let fixture = Fixture::new();
    let slots = [0, 1, 2];
    let mut store = FileNameStore::new(fixture.path.clone());
    for slot in slots {
        store.set_names(slot, names_for(slot));
    }
    rewrite(&mut store, slots.to_vec(), 1);
    assert_eq!(store.block_count(), 1, "every folder shares one block");

    store.begin_spill().expect("spill");
    store.spill_names(1, packed(&["spilled.txt"]));
    rewrite(&mut store, slots.to_vec(), 2);

    assert_eq!(current(&store, 0), Some(strings(&names_for(0))));
    assert_eq!(current(&store, 1), Some(vec!["spilled.txt".to_string()]));
    assert_eq!(current(&store, 2), Some(strings(&names_for(2))));
}

#[test]
fn spilling_without_a_spill_falls_back_to_the_overlay_and_stale_spills_are_removed() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    store.spill_names(3, packed(&["a"]));
    assert_eq!(current(&store, 3), Some(vec!["a".to_string()]));
    assert_eq!(store.overlay_stats().folders, 1);

    fs::create_dir_all(fixture.path.parent().expect("dir")).expect("dir");
    fs::write(store.spill_path(), b"left over by a crash").expect("stale");
    store.remove_stale_spill().expect("removed");
    assert!(!store.spill_path().exists());
    store.remove_stale_spill().expect("nothing to remove");

    store.begin_spill().expect("first");
    store.spill_names(1, packed(&["first"]));
    store.begin_spill().expect("restarted");
    assert_eq!(store.spill_len(), Some(0), "a restart begins empty");
    assert_eq!(current(&store, 1), None);
}

#[test]
fn open_pair_accepts_only_a_matching_token_and_folder_count() {
    let fixture = Fixture::new();
    let written = written_store(&fixture, &[0, 1, 2]);
    let header = written.names_header().expect("header");
    assert_eq!(
        header,
        NamesHeader {
            event_id: 900,
            pair_token: 1,
            folder_count: 3,
        }
    );
    drop(written);

    let mut store = fixture.store();
    let mismatched_token = PairExpectation {
        pair_token: 2,
        first_slot: FIRST_SLOT,
        folder_count: 3,
    };
    assert_eq!(store.open_pair(mismatched_token), None);
    let mismatched_count = PairExpectation {
        pair_token: 1,
        first_slot: FIRST_SLOT,
        folder_count: 4,
    };
    assert_eq!(store.open_pair(mismatched_count), None);
    assert!(!store.has_names_file());
    let matching = PairExpectation {
        pair_token: 1,
        first_slot: FIRST_SLOT,
        folder_count: 3,
    };
    assert_eq!(store.open_pair(matching), Some(header));
    assert_eq!(
        store.ordinal_of(2),
        Some(2),
        "slots equal ordinals after load"
    );
    assert_eq!(current(&store, 1), Some(strings(&names_for(1))));
}

#[test]
fn replacing_the_names_file_while_it_is_mapped_succeeds() {
    let fixture = Fixture::new();
    let mut store = written_store(&fixture, &[0, 1]);
    assert!(store.has_names_file());
    store.set_names(1, packed(&["replacement.txt"]));
    rewrite(&mut store, vec![0, 1], 2);
    assert_eq!(
        store.names_header().map(|header| header.pair_token),
        Some(2)
    );
    assert_eq!(
        current(&store, 1),
        Some(vec!["replacement.txt".to_string()])
    );
    rewrite(&mut store, vec![1], 3);
    assert_eq!(
        store.names_header().map(|header| header.folder_count),
        Some(1)
    );
    assert_eq!(
        current(&store, 1),
        Some(vec!["replacement.txt".to_string()])
    );
}

#[test]
fn a_corrupt_block_flips_availability_and_is_reported_once() {
    let fixture = Fixture::new();
    drop(written_store(&fixture, &[0, 1, 2, 3]));
    corrupt_byte_at(&fixture.path, HEADER_BYTES);
    let mut store = fixture.store();
    let expected = PairExpectation {
        pair_token: 1,
        first_slot: FIRST_SLOT,
        folder_count: 4,
    };
    assert!(store.open_pair(expected).is_some(), "the header is intact");
    assert!(store.is_available());
    let mut scratch = NamesScratch::default();
    assert!(store.read_block(0, &mut scratch).is_err());
    assert!(!store.is_available());
    assert!(store.take_unreported_failure());
    assert!(!store.take_unreported_failure(), "reported once");
    assert!(store.names_of(0).is_err());

    let capture = store.capture(vec![0, 1, 2, 3]);
    let rebuilt = write_names(capture, stamp(5)).expect("written without the broken file");
    store.install(rebuilt, || Ok(())).expect("installed");
    assert!(store.is_available(), "a fresh pair clears the failure");
    assert_eq!(current(&store, 2), Some(Vec::new()));

    store.mark_unreadable();
    assert!(!store.is_available());
    store.fail_and_discard();
    assert!(!store.has_names_file());
    assert!(!store.take_unreported_failure(), "already handled");
}

#[test]
fn rewrites_that_need_a_corrupt_block_fail_as_unreadable() {
    let fixture = Fixture::new();
    drop(written_store(&fixture, &[0, 1, 2, 3]));
    corrupt_byte_at(&fixture.path, HEADER_BYTES);
    let expected = PairExpectation {
        pair_token: 1,
        first_slot: FIRST_SLOT,
        folder_count: 4,
    };

    let mut copying = fixture.store();
    copying.open_pair(expected).expect("open");
    let copy_failure = write_names(copying.capture(vec![0, 1, 2, 3]), stamp(2));
    assert!(matches!(copy_failure, Err(RewriteError::Unreadable)));
    assert!(
        !temp_path(&fixture.path).exists(),
        "the temp file is discarded"
    );

    let mut reencoding = fixture.store();
    reencoding.open_pair(expected).expect("open");
    let reordered = write_names(reencoding.capture(vec![1, 0, 2, 3]), stamp(2));
    assert!(matches!(reordered, Err(RewriteError::Unreadable)));
}

#[test]
fn a_damaged_spill_fails_reads_and_rewrites() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    store.begin_spill().expect("spill");
    store.spill_names(0, packed(&["one"]));
    fs::write(store.spill_path(), b"").expect("truncate the spill");
    assert!(store.names_of(0).is_err());
    let mut scratch = NamesScratch::default();
    assert!(store.visit_spill(&mut scratch, &mut |_| {}).is_err());
    let failed = write_names(store.capture(vec![0]), stamp(1));
    assert!(matches!(failed, Err(RewriteError::Unreadable)));
}

#[test]
fn an_expired_deadline_abandons_the_rewrite_before_anything_is_renamed() {
    let fixture = Fixture::new();
    let mut store = written_store(&fixture, &[0, 1, 2]);
    store.set_names(1, packed(&["late.txt"]));
    let expired = PairStamp {
        deadline: Some(Instant::now()),
        ..stamp(2)
    };
    assert!(is_past(expired.deadline));
    assert!(!is_past(None));
    assert!(!is_past(Some(Instant::now() + Duration::from_secs(60))));
    let failed = write_names(store.capture(vec![0, 1, 2]), expired);
    assert!(matches!(failed, Err(RewriteError::DeadlinePassed)));
    let empty = write_names(store.capture(Vec::new()), expired);
    assert!(matches!(empty, Err(RewriteError::DeadlinePassed)));
    assert!(!temp_path(&fixture.path).exists());
    assert_eq!(
        store.names_header().map(|header| header.pair_token),
        Some(1)
    );
}

#[test]
fn install_failures_keep_a_consistent_store() {
    let fixture = Fixture::new();
    let mut store = written_store(&fixture, &[0, 1]);
    store.set_names(0, packed(&["kept.txt"]));

    let written = write_names(store.capture(vec![0, 1]), stamp(2)).expect("written");
    fs::remove_file(&written.temp).expect("lose the temp file");
    let lost = store.install(written, || Ok(()));
    assert!(matches!(lost, Err(InstallError::NamesNotReplaced(_))));
    assert_eq!(
        store.names_header().map(|header| header.pair_token),
        Some(1)
    );
    assert_eq!(current(&store, 0), Some(vec!["kept.txt".to_string()]));
    assert_eq!(current(&store, 1), Some(strings(&names_for(1))));

    let written = write_names(store.capture(vec![0, 1]), stamp(3)).expect("written");
    let snapshot_failed = store.install(written, || Err(io::Error::other("disk full")));
    assert!(matches!(
        snapshot_failed,
        Err(InstallError::SnapshotNotReplaced(_))
    ));
    assert_eq!(
        store.names_header().map(|header| header.pair_token),
        Some(3)
    );
    assert!(!store.has_unsaved_names());

    let blocked = fixture.path.parent().expect("dir").join("blocked");
    fs::write(&blocked, b"x").expect("file");
    let unwritable = FileNameStore::new(blocked.join(NAMES_FILE));
    let io_failure = write_names(unwritable.capture(vec![0]), stamp(1));
    assert!(matches!(io_failure, Err(RewriteError::Io(_))));
    assert!(matches!(
        RewriteError::from(io::Error::other("x")),
        RewriteError::Io(_)
    ));
}

#[test]
fn overlay_thresholds_count_folders_and_name_bytes() {
    assert_eq!(OVERLAY_THRESHOLD_BYTES, 4 * 1024 * 1024);
    assert_eq!(OVERLAY_THRESHOLD_FOLDERS, 5_000);
    let small = OverlayStats {
        folders: OVERLAY_THRESHOLD_FOLDERS,
        name_bytes: OVERLAY_THRESHOLD_BYTES,
    };
    assert!(!small.exceeds_threshold());
    assert!(OverlayStats {
        folders: OVERLAY_THRESHOLD_FOLDERS + 1,
        ..small
    }
    .exceeds_threshold());
    assert!(OverlayStats {
        name_bytes: OVERLAY_THRESHOLD_BYTES + 1,
        ..small
    }
    .exceeds_threshold());

    let mut stats = RewriteStats {
        blocks_reencoded: 1,
        blocks_copied: 2,
        peak_open_blocks: 1,
    };
    stats.absorb(RewriteStats {
        blocks_reencoded: 3,
        blocks_copied: 4,
        peak_open_blocks: 1,
    });
    assert_eq!(
        stats,
        RewriteStats {
            blocks_reencoded: 4,
            blocks_copied: 6,
            peak_open_blocks: 1,
        }
    );
}

#[test]
fn counting_names_adds_the_overlay_to_blocks_it_does_not_replace() {
    let fixture = Fixture::new();
    let mut store = written_store(&fixture, &[0, 1, 2, 3]);
    assert_eq!(store.count_saved_and_overlay_names(), Ok(12));

    store.set_names(2, packed(&["fresh.txt"]));
    store.tombstone(3);
    store.set_names(4, packed(&["new-a", "new-b"]));

    assert_eq!(store.count_saved_and_overlay_names(), Ok(3 + 3 + 1 + 2));
}

#[test]
fn counting_names_fails_on_a_corrupt_block() {
    let fixture = Fixture::new();
    drop(written_store(&fixture, &[0, 1, 2, 3]));
    corrupt_byte_at(&fixture.path, HEADER_BYTES);
    let mut store = fixture.store();
    let expected = PairExpectation {
        pair_token: 1,
        first_slot: FIRST_SLOT,
        folder_count: 4,
    };
    store.open_pair(expected).expect("the header is intact");

    assert_eq!(store.count_saved_and_overlay_names(), Err(NamesUnreadable));
    assert!(store.take_unreported_failure());
}
