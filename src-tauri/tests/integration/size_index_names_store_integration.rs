use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use file_explorer_lib::size::index::names::file::{NAMES_FILE, SPILL_SUFFIX};
use file_explorer_lib::size::index::names::writer::RewriteStats;
use file_explorer_lib::size::index::names::{
    names_in, FileNameStore, FolderNames, NamesCounters, NamesCursor, NamesScratch,
    NamesUnreadable, OverlayStats, PackedNames, SavedNames, OVERLAY_THRESHOLD_BYTES,
    OVERLAY_THRESHOLD_FOLDERS,
};
use file_explorer_lib::size::index::segment::{
    MappedSegment, NodeFlags, SegmentBuilder, SegmentEntry, SegmentTarget,
};
use tempfile::{tempdir, TempDir};

use crate::common::segment::paths_in;

const SMALL_BLOCK_TARGET: usize = 64;
const NAMES_HEADER_BYTES: u64 = 48;
const TOKEN: u64 = 3;
const ROOT_SLOT: u32 = 0;
const FOLDER_COUNT: u32 = 4;
const NAMES_PER_FOLDER: u64 = 3;

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
    dir: TempDir,
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("cache").join(NAMES_FILE);
        Self { dir, path }
    }

    fn store(&self) -> FileNameStore {
        FileNameStore::new(self.path.clone())
    }

    fn segment_directory(&self) -> PathBuf {
        self.dir.path().join("segments")
    }

    fn segment_names_file(&self) -> PathBuf {
        paths_in(&self.segment_directory()).names
    }
}

fn entry<'entry>(
    depth: usize,
    name: &'entry str,
    files: &'entry PackedNames,
) -> SegmentEntry<'entry> {
    SegmentEntry {
        depth,
        name,
        direct: 0,
        flags: NodeFlags::default(),
        file_names: files.as_bytes(),
    }
}

fn write_segment(directory: &Path, folder_count: u32) {
    let target = SegmentTarget {
        paths: paths_in(directory),
        token: TOKEN,
    };
    let mut builder =
        SegmentBuilder::with_block_target(target, SMALL_BLOCK_TARGET).expect("builder");
    builder
        .push(entry(0, "root", &names_for(ROOT_SLOT)))
        .expect("root");
    for slot in 1..folder_count {
        let name = format!("folder-{slot:04}");
        builder
            .push(entry(1, &name, &names_for(slot)))
            .expect("child");
    }
    builder.finish().expect("finish");
}

fn saved_names(directory: &Path) -> SavedNames {
    let segment = MappedSegment::open(&paths_in(directory)).expect("opens");
    let mut saved = SavedNames::default();
    saved.attach_segment(0..segment.node_count(), segment.shared_file_names());
    saved
}

fn segment_store(fixture: &Fixture, folder_count: u32) -> FileNameStore {
    write_segment(&fixture.segment_directory(), folder_count);
    let mut store = fixture.store();
    store.restore(saved_names(&fixture.segment_directory()));
    store
}

fn corrupt_segment_store(fixture: &Fixture) -> FileNameStore {
    write_segment(&fixture.segment_directory(), FOLDER_COUNT);
    corrupt_byte_at(&fixture.segment_names_file(), NAMES_HEADER_BYTES);
    let mut store = fixture.store();
    store.restore(saved_names(&fixture.segment_directory()));
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

fn every_block_folder(store: &FileNameStore) -> Vec<u32> {
    (0..store.block_count())
        .flat_map(|index| block_folders(store, index))
        .collect()
}

fn corrupt_byte_at(path: &Path, offset: u64) {
    let mut file = OpenOptions::new().write(true).open(path).expect("open");
    file.seek(SeekFrom::Start(offset)).expect("seek");
    file.write_all(&[0xEE, 0xEE, 0xEE, 0xEE]).expect("corrupt");
}

fn rewritten(store: &FileNameStore, slot: u32, cursor: &mut NamesCursor) -> Vec<String> {
    let packed = store.names_for_rewrite(slot, cursor).expect("readable");
    names_in(packed)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
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
fn a_fresh_store_has_no_names_until_a_segment_or_overlay_exists() {
    let fixture = Fixture::new();
    let store = fixture.store();
    assert_eq!(store.path(), fixture.path);
    assert!(!store.is_available());
    assert!(!store.has_saved_names());
    assert_eq!(store.block_count(), 0);
    assert!(store.blocks_for_slots(&[1, 2]).is_empty());
    assert_eq!(store.names_of(4), Ok(None));
    assert!(!store.has_unsaved_changes());
    assert_eq!(store.counters(), NamesCounters::default());
    let mut scratch = NamesScratch::default();
    assert!(store.read_block(0, &mut scratch).is_err());
    let mut cursor = NamesCursor::default();
    assert!(rewritten(&store, 4, &mut cursor).is_empty());
}

#[test]
fn the_overlay_beats_blocks_and_tombstones_hide_pruned_folders() {
    let fixture = Fixture::new();
    let mut store = segment_store(&fixture, FOLDER_COUNT);
    assert!(store.is_available() && store.has_saved_names());
    assert!(!store.has_unsaved_changes());
    assert_eq!(current(&store, 2), Some(strings(&names_for(2))));
    assert_eq!(current(&store, FOLDER_COUNT), None, "past the segment");

    store.set_names(2, packed(&["fresh.txt"]));
    store.tombstone(3);
    assert_eq!(current(&store, 2), Some(vec!["fresh.txt".to_string()]));
    assert_eq!(current(&store, 3), Some(Vec::new()), "pruned");
    assert!(store.has_unsaved_changes());
    store.mark_saved();
    assert!(!store.has_unsaved_changes());
    assert_eq!(
        store.overlay_stats(),
        OverlayStats {
            folders: 2,
            name_bytes: "fresh.txt\0".len(),
        }
    );
    assert_eq!(
        every_block_folder(&store),
        [0, 1],
        "blocks skip folders the overlay replaced"
    );
    let mut overlay = Vec::new();
    store.visit_overlay(&mut |folder| overlay.push(folder.slot));
    assert_eq!(overlay, [2], "tombstones carry no names");

    store.set_names(3, packed(&["reused.slot"]));
    assert_eq!(current(&store, 3), Some(vec!["reused.slot".to_string()]));
    store.set_names(3, packed(&["x"]));
    assert_eq!(store.overlay_stats().name_bytes, "fresh.txt\0x\0".len());
}

#[test]
fn a_dead_range_hides_its_folders_and_drops_their_overlay_names() {
    let fixture = Fixture::new();
    let mut store = segment_store(&fixture, FOLDER_COUNT);
    store.set_names(2, packed(&["fresh.txt"]));
    store.mark_saved();
    store.tombstone_range(1..3);
    assert!(store.has_unsaved_changes());
    assert_eq!(current(&store, 1), Some(Vec::new()));
    assert_eq!(current(&store, 2), Some(Vec::new()));
    assert_eq!(current(&store, 3), Some(strings(&names_for(3))));
    assert_eq!(store.overlay_stats(), OverlayStats::default());
    assert_eq!(every_block_folder(&store), [0, 3]);
    assert_eq!(
        store.count_saved_and_overlay_names(),
        Ok(2 * NAMES_PER_FOLDER)
    );
    let mut cursor = NamesCursor::default();
    assert!(rewritten(&store, 2, &mut cursor).is_empty());
}

#[test]
fn small_blocks_split_the_folders_and_each_slot_maps_to_its_block() {
    let fixture = Fixture::new();
    let folder_count = 12;
    let store = segment_store(&fixture, folder_count);
    assert!(store.block_count() > 3, "roughly one block per folder");
    assert_eq!(
        every_block_folder(&store),
        (0..folder_count).collect::<Vec<u32>>()
    );
    let first_and_last = store.blocks_for_slots(&[folder_count - 1, 0, 0, folder_count]);
    assert_eq!(first_and_last, [0, store.block_count() - 1]);
    for block in 0..store.block_count() {
        let folders = block_folders(&store, block);
        assert_eq!(store.blocks_for_slots(&folders), [block]);
    }
}

#[test]
fn a_rewrite_cursor_reads_every_source_and_decodes_each_block_once() {
    let fixture = Fixture::new();
    let folder_count = 12;
    let mut store = segment_store(&fixture, folder_count);
    store.set_names(5, packed(&["overlay.txt"]));
    store.tombstone(6);
    let new_slot = folder_count + 20;
    store.set_names(new_slot, packed(&["brand-new.txt"]));

    let mut cursor = NamesCursor::default();
    for slot in 0..folder_count {
        let expected = match slot {
            5 => vec!["overlay.txt".to_string()],
            6 => Vec::new(),
            _ => strings(&names_for(slot)),
        };
        assert_eq!(
            rewritten(&store, slot, &mut cursor),
            expected,
            "slot {slot}"
        );
    }
    assert_eq!(rewritten(&store, new_slot, &mut cursor), ["brand-new.txt"]);
    assert!(rewritten(&store, new_slot + 1, &mut cursor).is_empty());
    let decoded = store.counters().blocks_decompressed;
    assert!(decoded <= store.block_count() as u64, "{decoded} decodes");
    assert_eq!(rewritten(&store, 0, &mut cursor), strings(&names_for(0)));
}

#[test]
fn an_attached_segment_serves_the_names_of_its_own_id_range() {
    let fixture = Fixture::new();
    let mut store = segment_store(&fixture, FOLDER_COUNT);
    store.mark_saved();
    let graft_directory = fixture.dir.path().join("graft");
    write_segment(&graft_directory, FOLDER_COUNT);
    let graft = MappedSegment::open(&paths_in(&graft_directory)).expect("opens");
    let graft_ids = FOLDER_COUNT..FOLDER_COUNT + graft.node_count();
    store.attach_segment(graft_ids.clone(), graft.shared_file_names());
    assert!(
        store.has_unsaved_changes(),
        "the manifest must list the graft"
    );
    assert_eq!(current(&store, 1), Some(strings(&names_for(1))));
    assert_eq!(
        current(&store, graft_ids.start + 2),
        Some(strings(&names_for(2))),
        "ordinal 2 of the graft"
    );
    assert_eq!(current(&store, graft_ids.end), None, "past every segment");
    assert_eq!(
        store.count_saved_and_overlay_names(),
        Ok(2 * u64::from(FOLDER_COUNT) * NAMES_PER_FOLDER)
    );

    store.tombstone_range(graft_ids.start + 1..graft_ids.start + 2);
    assert_eq!(current(&store, graft_ids.start + 1), Some(Vec::new()));
    let mut cursor = NamesCursor::default();
    assert_eq!(
        rewritten(&store, graft_ids.start + 3, &mut cursor),
        strings(&names_for(3))
    );
}

#[test]
fn tombstoning_a_folder_without_saved_names_only_drops_its_overlay_entry() {
    let fixture = Fixture::new();
    let mut store = segment_store(&fixture, FOLDER_COUNT);
    let heap_slot = FOLDER_COUNT + 9;
    store.set_names(heap_slot, packed(&["a"]));
    assert_eq!(store.overlay_stats().folders, 1);
    store.tombstone(heap_slot);
    assert_eq!(store.overlay_stats(), OverlayStats::default());
    assert_eq!(current(&store, heap_slot), None);
    store.tombstone(1);
    assert_eq!(
        store.overlay_stats().folders,
        1,
        "a saved folder needs a tombstone"
    );
    assert_eq!(current(&store, 1), Some(Vec::new()));
}

#[test]
fn legacy_files_from_older_formats_are_removed() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let directory = fixture.path.parent().expect("dir");
    fs::create_dir_all(directory).expect("dir");
    let legacy_spill = directory.join(format!("{NAMES_FILE}{SPILL_SUFFIX}"));
    fs::write(&fixture.path, b"old names").expect("legacy names");
    fs::write(&legacy_spill, b"left over by a crash").expect("legacy spill");
    store.remove_legacy_files().expect("removed");
    assert!(!fixture.path.exists());
    assert!(!legacy_spill.exists());
    store.remove_legacy_files().expect("nothing to remove");
}

#[test]
fn a_corrupt_block_flips_availability_and_is_reported_once() {
    let fixture = Fixture::new();
    let mut store = corrupt_segment_store(&fixture);
    assert!(store.is_available(), "the header is intact");
    let mut scratch = NamesScratch::default();
    assert!(store.read_block(0, &mut scratch).is_err());
    assert!(!store.is_available());
    assert!(store.take_unreported_failure());
    assert!(!store.take_unreported_failure(), "reported once");
    assert!(store.names_of(0).is_err());
    let mut cursor = NamesCursor::default();
    assert_eq!(
        store.names_for_rewrite(0, &mut cursor),
        Err(NamesUnreadable)
    );

    store.adopt_base(SavedNames::default(), RewriteStats::default());
    assert!(!store.is_available(), "nothing saved yet");
    store.set_names(0, packed(&["fresh"]));
    assert!(store.is_available(), "a fresh base clears the failure");

    store.mark_unreadable();
    assert!(!store.is_available());
    store.fail_and_discard();
    assert!(!store.has_saved_names());
    assert!(!store.take_unreported_failure(), "already handled");
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
        peak_open_blocks: 1,
    };
    stats.absorb(RewriteStats {
        blocks_reencoded: 3,
        peak_open_blocks: 1,
    });
    assert_eq!(
        stats,
        RewriteStats {
            blocks_reencoded: 4,
            peak_open_blocks: 1,
        }
    );
}

#[test]
fn counting_names_adds_the_overlay_to_blocks_it_does_not_replace() {
    let fixture = Fixture::new();
    let mut store = segment_store(&fixture, FOLDER_COUNT);
    assert_eq!(
        store.count_saved_and_overlay_names(),
        Ok(u64::from(FOLDER_COUNT) * NAMES_PER_FOLDER)
    );

    store.set_names(2, packed(&["fresh.txt"]));
    store.tombstone(3);
    store.set_names(FOLDER_COUNT, packed(&["new-a", "new-b"]));

    assert_eq!(store.count_saved_and_overlay_names(), Ok(3 + 3 + 1 + 2));
}

#[test]
fn counting_names_fails_on_a_corrupt_block() {
    let fixture = Fixture::new();
    let store = corrupt_segment_store(&fixture);
    assert_eq!(store.count_saved_and_overlay_names(), Err(NamesUnreadable));
    assert!(store.take_unreported_failure());
}
