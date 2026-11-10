use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use file_explorer_lib::size::index::names::{FileNameStore, PackedNames, SavedNames};
use file_explorer_lib::size::index::persist::{
    decode, encode, load, plan_batch, plan_catch_up, save_encoded, temp_path, write_temp,
    ChangeEvent, ChangeFlags, CurrentVolume, Manifest, ReplayOutcome, SegmentRef, Snapshot,
    SnapshotMeta, FORMAT_VERSION,
};
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{
    strings, walked, walked_segment, SegmentPlace, Walked, FIRST_SEQUENCE,
};

fn path_of(parts: &[&str]) -> PathBuf {
    std::iter::once("idx")
        .chain(parts.iter().copied())
        .collect()
}

const BUILD: &str = "25A123";
const TOKEN: u64 = 77;
const OVERLAY_ID_BIT: u32 = 1 << 31;
const NO_PARENT: u32 = u32::MAX;
const SCANNED: u8 = 1;
const UNKNOWN_FLAG: u8 = 0x80;
const NO_FILE_NAMES: u8 = 0;
const NO_NAMES_PURGE_DUE: u8 = 0;
const NO_FULL_SCAN_RUNNING: u8 = 0;
const NAMES_FILE: &str = "unused-names.bin";

fn meta() -> SnapshotMeta {
    SnapshotMeta {
        volume_uuid: "0A1B-UUID".into(),
        os_build: BUILD.into(),
        event_id: 500,
        last_full_scan_secs: 1_760_000_000,
        visited_roots: vec![path_of(&["home", "Desktop"])],
        has_file_names: true,
        names_purge_due: true,
        full_scan_running: true,
        exclusions: vec![
            path_of(&["build-cache"]),
            path_of(&["home", "node_modules"]),
        ],
    }
}

fn sample_tree() -> FolderSizeTree {
    let mut tree = FolderSizeTree::default();
    tree.add_root(&path_of(&[]));
    tree.add_root(&["second"].iter().collect::<PathBuf>());
    tree.merge_record(
        &path_of(&[]),
        DirectoryListing::scanned(1, &["home", "pending"]),
    );
    tree.merge_record(
        &path_of(&["home"]),
        DirectoryListing::scanned(20, &["zeta", "alpha", "locked", "Desktop"]),
    );
    tree.merge_record::<&str>(
        &path_of(&["home", "zeta"]),
        DirectoryListing::scanned(300, &[]),
    );
    tree.merge_record::<&str>(
        &path_of(&["home", "alpha"]),
        DirectoryListing::scanned(4000, &[]),
    );
    tree.merge_record::<&str>(
        &path_of(&["home", "locked"]),
        DirectoryListing {
            direct_bytes: 0,
            subdirs: &[],
            denied: true,
        },
    );
    tree.set_skipped(&path_of(&["home", "Desktop"]), true);
    tree.merge_record::<&str>(
        &path_of(&["pending", "child"]),
        DirectoryListing::scanned(7, &[]),
    );
    tree.merge_record::<&str>(
        &["second", "x"].iter().collect::<PathBuf>(),
        DirectoryListing::scanned(9, &[]),
    );
    tree.prune(&path_of(&["home", "zeta"]));
    tree
}

fn names_store() -> FileNameStore {
    FileNameStore::new(PathBuf::from(NAMES_FILE))
}

fn sample_names(tree: &FolderSizeTree) -> FileNameStore {
    let mut names = names_store();
    let home = tree.lookup(&path_of(&["home"])).expect("home");
    let alpha = tree.lookup(&path_of(&["home", "alpha"])).expect("alpha");
    names.set_names(home, PackedNames::from_names(["notes.txt", "todo.md"]));
    names.set_names(alpha, PackedNames::default());
    names
}

fn encoded(tree: &FolderSizeTree, names: &FileNameStore) -> Vec<u8> {
    encode(&Manifest {
        meta: &meta(),
        segments: &[],
        tree,
        names,
    })
}

fn sample_bytes() -> Vec<u8> {
    let tree = sample_tree();
    encoded(&tree, &sample_names(&tree))
}

fn decoded(bytes: &[u8]) -> Option<Snapshot> {
    decode(bytes, Path::new("no-segments-here"))
}

fn restored_names(
    snapshot_names: file_explorer_lib::size::index::names::SavedNames,
) -> FileNameStore {
    let mut names = names_store();
    names.restore(snapshot_names);
    names
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, value: &[u8]) {
    put_u32(out, value.len() as u32);
    out.extend_from_slice(value);
}

fn put_ids(out: &mut Vec<u8>, ids: &[u32]) {
    put_u32(out, ids.len() as u32);
    for &id in ids {
        put_u32(out, id);
    }
}

fn current<'volume>(
    uuid: &'volume str,
    os_build: &'volume str,
    event_id: u64,
) -> CurrentVolume<'volume> {
    CurrentVolume {
        uuid,
        os_build,
        event_id,
    }
}

fn overlay_id(index: u32) -> u32 {
    OVERLAY_ID_BIT | index
}

struct CraftedFolder {
    id: u32,
    parent: u32,
    name: &'static [u8],
    flags: u8,
    children: Vec<u32>,
}

fn root_folder() -> CraftedFolder {
    CraftedFolder {
        id: overlay_id(0),
        parent: NO_PARENT,
        name: b"idx",
        flags: SCANNED,
        children: Vec::new(),
    }
}

struct Crafted {
    version: u32,
    visited: u32,
    exclusions: u32,
    segments: u32,
    roots: Vec<u32>,
    folders: Vec<CraftedFolder>,
    free_slots: Vec<u32>,
    patched_ids: Vec<u32>,
    dead_ranges: Vec<(u32, u32)>,
    names: Vec<u8>,
    trailing: Vec<u8>,
}

fn crafted() -> Crafted {
    Crafted {
        version: FORMAT_VERSION,
        visited: 0,
        exclusions: 0,
        segments: 0,
        roots: vec![overlay_id(0)],
        folders: vec![root_folder()],
        free_slots: Vec::new(),
        patched_ids: Vec::new(),
        dead_ranges: Vec::new(),
        names: 0_u32.to_le_bytes().to_vec(),
        trailing: Vec::new(),
    }
}

impl Crafted {
    fn put_folder(out: &mut Vec<u8>, folder: &CraftedFolder) {
        put_u32(out, folder.id);
        put_u32(out, folder.parent);
        put_str(out, folder.name);
        out.extend_from_slice(&5_u64.to_le_bytes());
        out.push(folder.flags);
        out.extend_from_slice(&[0; 20]);
        put_ids(out, &folder.children);
    }

    fn put_patch(out: &mut Vec<u8>, id: u32) {
        put_u32(out, id);
        out.extend_from_slice(&[0; 20]);
        out.extend_from_slice(&[0, 0, 0]);
    }

    fn bytes(&self) -> Vec<u8> {
        let mut out = b"AXOSIZE\0".to_vec();
        put_u32(&mut out, self.version);
        put_str(&mut out, b"uuid");
        put_str(&mut out, b"build");
        out.extend_from_slice(&[0; 16]);
        put_u32(&mut out, self.visited);
        out.extend([NO_FILE_NAMES, NO_NAMES_PURGE_DUE, NO_FULL_SCAN_RUNNING]);
        put_u32(&mut out, self.exclusions);
        put_u32(&mut out, self.segments);
        put_ids(&mut out, &self.roots);
        put_u32(&mut out, self.folders.len() as u32);
        for folder in &self.folders {
            Self::put_folder(&mut out, folder);
        }
        put_ids(&mut out, &self.free_slots);
        put_u32(&mut out, self.patched_ids.len() as u32);
        for &id in &self.patched_ids {
            Self::put_patch(&mut out, id);
        }
        put_u32(&mut out, self.dead_ranges.len() as u32);
        for &(start, end) in &self.dead_ranges {
            put_u32(&mut out, start);
            put_u32(&mut out, end);
        }
        out.extend_from_slice(&self.names);
        put_u32(&mut out, 0);
        out.extend_from_slice(&self.trailing);
        let checksum = fnv1a(&out);
        out.extend_from_slice(&checksum.to_le_bytes());
        out
    }

    fn decodes(&self) -> bool {
        decoded(&self.bytes()).is_some()
    }
}

const COMPARED_FOLDERS: [&[&str]; 7] = [
    &[],
    &["home"],
    &["home", "alpha"],
    &["home", "locked"],
    &["home", "Desktop"],
    &["home", "zeta"],
    &["pending"],
];

fn assert_same_totals(restored: &FolderSizeTree, saved: &FolderSizeTree) {
    for parts in COMPARED_FOLDERS {
        assert_eq!(
            restored.total(&path_of(parts)),
            saved.total(&path_of(parts)),
            "{parts:?}"
        );
    }
    assert_eq!(restored.node_count(), saved.node_count());
    assert_eq!(restored.unscanned_tops(), saved.unscanned_tops());
}

#[test]
fn round_trip_is_byte_exact_and_restores_totals_and_overlay_names() {
    let tree = sample_tree();
    let names = sample_names(&tree);
    let bytes = encoded(&tree, &names);
    let snapshot = decoded(&bytes).expect("decodes");
    assert_eq!(snapshot.meta, meta());
    assert!(snapshot.segments.is_empty());
    assert!(!snapshot.tree.is_dirty());
    assert_same_totals(&snapshot.tree, &tree);
    let second: PathBuf = ["second"].iter().collect();
    assert_eq!(
        snapshot.tree.total(&second).map(|total| total.bytes),
        Some(9)
    );
    let home = snapshot.tree.total(&path_of(&["home"])).expect("home");
    assert_eq!(home.bytes, 4020);
    assert_eq!(home.lower_bound.expect("bound").denied_count, 1);
    // Pending placeholders survive, so an interrupted scan can resume.
    assert!(!snapshot.tree.unscanned_tops().is_empty());

    let home_slot = snapshot.tree.lookup(&path_of(&["home"])).expect("home");
    let alpha_slot = snapshot.tree.lookup(&path_of(&["home", "alpha"]));
    let restored = restored_names(snapshot.names);
    assert_eq!(encoded(&snapshot.tree, &restored), bytes);
    assert!(!restored.has_unsaved_changes());
    assert_eq!(
        restored.names_of(home_slot),
        Ok(Some(PackedNames::from_names(["notes.txt", "todo.md"])))
    );
    assert_eq!(
        restored.names_of(alpha_slot.expect("alpha")),
        Ok(Some(PackedNames::default()))
    );
}

#[test]
fn a_freed_overlay_slot_is_reused_after_a_round_trip() {
    let mut restored = decoded(&sample_bytes()).expect("decodes").tree;
    let before = restored.heap_ids();
    assert!(restored.merge_record::<&str>(
        &path_of(&["home", "again"]),
        DirectoryListing::scanned(1, &[])
    ));
    assert_eq!(restored.heap_ids(), before, "the pruned folder's slot");
}

struct SegmentBacked {
    directory: TempDir,
    segments: Vec<SegmentRef>,
    tree: FolderSizeTree,
    names: FileNameStore,
}

fn base_walk() -> Vec<Walked> {
    vec![
        Walked {
            subdirs: strings(&["home", "pending"]),
            files: strings(&["root.txt"]),
            ..walked(path_of(&[]), 1)
        },
        Walked {
            subdirs: strings(&["alpha", "zeta", "locked", "Desktop"]),
            files: strings(&["home.txt"]),
            ..walked(path_of(&["home"]), 20)
        },
        Walked {
            files: strings(&["a1", "a2"]),
            ..walked(path_of(&["home", "alpha"]), 4000)
        },
        walked(path_of(&["home", "zeta"]), 300),
        Walked {
            denied: true,
            ..walked(path_of(&["home", "locked"]), 0)
        },
    ]
}

fn segment_backed() -> SegmentBacked {
    let directory = tempdir().expect("temp dir");
    let place = SegmentPlace {
        directory: directory.path(),
        root: &path_of(&[]),
        token: TOKEN,
    };
    let segment = Arc::new(walked_segment(place, &base_walk()));
    let mut tree = FolderSizeTree::default();
    let slots = tree.attach_base(Arc::clone(&segment)).expect("attached");
    let mut base_names = SavedNames::default();
    base_names.attach_segment(slots, segment.shared_file_names());
    let mut names = names_store();
    names.restore(base_names);

    let home = tree.lookup(&path_of(&["home"])).expect("home");
    tree.apply_listing(
        &path_of(&["home"]),
        DirectoryListing::scanned(25, &["alpha", "locked", "Desktop", "added"]),
    );
    for slots in tree.take_removed_ranges() {
        names.tombstone_range(slots);
    }
    names.set_names(home, PackedNames::from_names(["home-v2.txt"]));
    tree.merge_record::<&str>(
        &path_of(&["home", "added", "deeper"]),
        DirectoryListing::scanned(6, &[]),
    );
    tree.set_skipped(&path_of(&["home", "Desktop"]), true);
    SegmentBacked {
        segments: vec![SegmentRef {
            sequence: FIRST_SEQUENCE,
            token: TOKEN,
        }],
        directory,
        tree,
        names,
    }
}

impl SegmentBacked {
    fn bytes(&self) -> Vec<u8> {
        encode(&Manifest {
            meta: &meta(),
            segments: &self.segments,
            tree: &self.tree,
            names: &self.names,
        })
    }

    fn names_of(names: &FileNameStore, tree: &FolderSizeTree, parts: &[&str]) -> Vec<String> {
        let slot = tree.lookup(&path_of(parts)).expect("folder");
        let packed = names.names_of(slot).expect("readable").unwrap_or_default();
        packed
            .names()
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect()
    }
}

#[test]
fn a_manifest_over_a_base_segment_restores_patches_dead_ranges_and_names() {
    let saved = segment_backed();
    let bytes = saved.bytes();
    let snapshot = decode(&bytes, saved.directory.path()).expect("decodes");
    assert_eq!(snapshot.segments, saved.segments);
    assert_same_totals(&snapshot.tree, &saved.tree);
    assert_eq!(
        snapshot.tree.total(&path_of(&["home", "added", "deeper"])),
        saved.tree.total(&path_of(&["home", "added", "deeper"]))
    );
    assert_eq!(snapshot.tree.overlay_stats(), saved.tree.overlay_stats());
    assert!(snapshot.tree.total(&path_of(&["home", "zeta"])).is_none());

    let restored = restored_names(snapshot.names);
    assert!(restored.is_available());
    assert_eq!(
        SegmentBacked::names_of(&restored, &snapshot.tree, &["home"]),
        ["home-v2.txt"]
    );
    assert_eq!(
        SegmentBacked::names_of(&restored, &snapshot.tree, &["home", "alpha"]),
        ["a1", "a2"]
    );
    assert_eq!(restored.count_saved_and_overlay_names(), Ok(4));
    assert_eq!(
        encode(&Manifest {
            meta: &snapshot.meta,
            segments: &snapshot.segments,
            tree: &snapshot.tree,
            names: &restored,
        }),
        bytes
    );
}

#[test]
fn a_manifest_needs_every_segment_it_names_with_the_recorded_token() {
    let saved = segment_backed();
    let bytes = saved.bytes();
    assert!(decoded(&bytes).is_none(), "segment files are elsewhere");

    let other_token = vec![SegmentRef {
        sequence: FIRST_SEQUENCE,
        token: TOKEN + 1,
    }];
    let mismatched = encode(&Manifest {
        meta: &meta(),
        segments: &other_token,
        tree: &saved.tree,
        names: &saved.names,
    });
    assert!(decode(&mismatched, saved.directory.path()).is_none());

    let path = saved.directory.path().join("folder-size-index.bin");
    save_encoded(&path, &bytes).expect("saved");
    assert_eq!(load(&path).expect("loads").segments, saved.segments);
}

#[test]
fn save_is_atomic_and_load_reads_it_back() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("cache").join("size-index.bin");
    let empty = encoded(&FolderSizeTree::default(), &names_store());
    save_encoded(&path, &empty).expect("first save");
    save_encoded(&path, &sample_bytes()).expect("overwrite");
    assert_eq!(fs::read(&path).expect("written"), sample_bytes());
    assert!(!path.with_extension("tmp").exists());
    assert_eq!(load(&path).expect("loads").meta, meta());

    assert!(load(&dir.path().join("missing.bin")).is_none());
    let blocked = dir.path().join("file");
    fs::write(&blocked, b"x").expect("file");
    assert!(save_encoded(&blocked.join("snap.bin"), &sample_bytes()).is_err());
}

#[test]
fn any_corrupted_byte_or_truncation_yields_none() {
    let bytes = sample_bytes();
    for at in 0..bytes.len() {
        let mut corrupt = bytes.clone();
        corrupt[at] ^= 0x41;
        assert!(decoded(&corrupt).is_none(), "flip at {at}");
    }
    for len in 0..bytes.len() {
        assert!(decoded(&bytes[..len]).is_none(), "truncated to {len}");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(decoded(&longer).is_none());
}

#[test]
fn other_versions_and_oversized_lists_yield_none_even_with_a_valid_checksum() {
    assert!(crafted().decodes(), "control");
    assert_eq!(FORMAT_VERSION, 4);
    for version in [FORMAT_VERSION - 1, FORMAT_VERSION + 1] {
        let other_version = Crafted {
            version,
            ..crafted()
        };
        assert!(!other_version.decodes(), "upgrading users rescan once");
    }
    let trailing = Crafted {
        trailing: vec![0],
        ..crafted()
    };
    assert!(!trailing.decodes());
    let too_many_visited = Crafted {
        visited: 2_000,
        ..crafted()
    };
    assert!(!too_many_visited.decodes());
    let too_many_exclusions = Crafted {
        exclusions: 2_000,
        ..crafted()
    };
    assert!(!too_many_exclusions.decodes());
    let too_many_segments = Crafted {
        segments: 2_000,
        ..crafted()
    };
    assert!(!too_many_segments.decodes());
    let missing_segment = Crafted {
        segments: 1,
        ..crafted()
    };
    assert!(!missing_segment.decodes());
}

fn with_root(root: CraftedFolder) -> Crafted {
    Crafted {
        folders: vec![root],
        ..crafted()
    }
}

#[test]
fn overlay_folders_that_do_not_link_up_yield_none() {
    let nameless = with_root(CraftedFolder {
        name: b"",
        ..root_folder()
    });
    assert!(!nameless.decodes());
    let unknown_flag = with_root(CraftedFolder {
        flags: UNKNOWN_FLAG,
        ..root_folder()
    });
    assert!(!unknown_flag.decodes());
    let not_text = with_root(CraftedFolder {
        name: &[0xC3, 0x28],
        ..root_folder()
    });
    assert!(!not_text.decodes());
    let orphan = with_root(CraftedFolder {
        parent: overlay_id(7),
        ..root_folder()
    });
    assert!(!orphan.decodes());
    let missing_child = with_root(CraftedFolder {
        children: vec![overlay_id(3)],
        ..root_folder()
    });
    assert!(!missing_child.decodes());
    let slot_out_of_range = with_root(CraftedFolder {
        id: overlay_id(5),
        ..root_folder()
    });
    assert!(!slot_out_of_range.decodes());
    let mapped_id_as_overlay_folder = with_root(CraftedFolder {
        id: 0,
        ..root_folder()
    });
    assert!(!mapped_id_as_overlay_folder.decodes());

    let unknown_root = Crafted {
        roots: vec![overlay_id(0), 9],
        ..crafted()
    };
    assert!(!unknown_root.decodes());
    let free_slot_in_use = Crafted {
        free_slots: vec![overlay_id(0)],
        ..crafted()
    };
    assert!(!free_slot_in_use.decodes());
    let spare_slot = Crafted {
        free_slots: vec![overlay_id(1)],
        ..crafted()
    };
    assert!(spare_slot.decodes(), "a free slot beside the root");
}

#[test]
fn patches_and_dead_ranges_must_point_into_a_segment() {
    let patch_without_segment = Crafted {
        patched_ids: vec![3],
        ..crafted()
    };
    assert!(!patch_without_segment.decodes());
    let dead_without_segment = Crafted {
        dead_ranges: vec![(1, 4)],
        ..crafted()
    };
    assert!(!dead_without_segment.decodes());
    let backwards_range = Crafted {
        dead_ranges: vec![(4, 4)],
        ..crafted()
    };
    assert!(!backwards_range.decodes());
}

fn overlay_names(has_names: u8, packed: &[u8]) -> Vec<u8> {
    let mut out = 1_u32.to_le_bytes().to_vec();
    put_u32(&mut out, overlay_id(0));
    out.push(has_names);
    put_str(&mut out, packed);
    out
}

#[test]
fn overlay_names_must_be_a_packed_list_or_an_empty_tombstone() {
    let cases = [
        (overlay_names(1, b"a.txt\0b.txt\0"), true),
        (overlay_names(1, b""), true),
        (overlay_names(0, b""), true),
        (overlay_names(0, b"a.txt\0"), false),
        (overlay_names(1, b"unterminated"), false),
        (overlay_names(1, b"\0\0"), false),
    ];
    for (names, accepted) in cases {
        let manifest = Crafted {
            names: names.clone(),
            ..crafted()
        };
        assert_eq!(manifest.decodes(), accepted, "{names:?}");
    }
}

#[test]
fn file_name_presence_and_exclusions_round_trip() {
    let saved = meta();
    let decoded_meta = decoded(&sample_bytes()).expect("decodes").meta;
    assert!(decoded_meta.has_file_names);
    assert!(decoded_meta.names_purge_due);
    assert!(decoded_meta.full_scan_running);
    assert_eq!(decoded_meta.exclusions, saved.exclusions);
    assert_eq!(decoded_meta.os_build, BUILD);
    let empty = SnapshotMeta::default();
    assert!(!empty.has_file_names && empty.exclusions.is_empty());
    let empty_bytes = encode(&Manifest {
        meta: &empty,
        segments: &[],
        tree: &FolderSizeTree::default(),
        names: &names_store(),
    });
    assert_eq!(decoded(&empty_bytes).expect("decodes").meta, empty);
}

#[test]
fn write_temp_leaves_the_target_alone_until_renamed() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("cache").join("size-index.bin");
    let empty = encoded(&FolderSizeTree::default(), &names_store());
    save_encoded(&path, &empty).expect("first save");
    let bytes = sample_bytes();
    let temp = write_temp(&path, &bytes).expect("temp written");
    assert_eq!(temp, temp_path(&path));
    assert_eq!(fs::read(&path).expect("unchanged"), empty);
    assert_eq!(fs::read(&temp).expect("temp"), bytes);
    save_encoded(&path, &bytes).expect("encoded save");
    assert_eq!(load(&path).expect("loads").meta, meta());
    assert!(!temp.exists());
}

fn event(parts: &[&str], flags: ChangeFlags) -> ChangeEvent {
    ChangeEvent {
        path: path_of(parts),
        flags,
    }
}

fn plain(parts: &[&str]) -> ChangeEvent {
    event(parts, ChangeFlags::default())
}

fn must_scan(parts: &[&str]) -> ChangeEvent {
    event(
        parts,
        ChangeFlags {
            must_scan_sub_dirs: true,
            ..ChangeFlags::default()
        },
    )
}

#[test]
fn catch_up_identical_id_is_up_to_date() {
    assert_eq!(
        plan_catch_up(&meta(), current("0A1B-UUID", BUILD, 500), &[],),
        ReplayOutcome::UpToDate
    );
}

#[test]
fn catch_up_advanced_id_rereads_reported_directories() {
    let events = [plain(&["b"]), plain(&["a"]), plain(&["b"])];
    assert_eq!(
        plan_catch_up(&meta(), current("0A1B-UUID", BUILD, 900), &events,),
        ReplayOutcome::Apply {
            reread: vec![path_of(&["a"]), path_of(&["b"])],
            rescan: vec![],
        }
    );
    assert_eq!(
        plan_catch_up(&meta(), current("0A1B-UUID", BUILD, 900), &[],),
        ReplayOutcome::UpToDate
    );
}

#[test]
fn catch_up_falls_back_to_a_full_rescan() {
    let saved = meta();
    let uuid = "0A1B-UUID";
    assert_eq!(
        plan_catch_up(&saved, current("OTHER", BUILD, 900), &[],),
        ReplayOutcome::FullRescan
    );
    assert_eq!(
        plan_catch_up(&saved, current(uuid, BUILD, 499), &[],),
        ReplayOutcome::FullRescan
    );
    assert_eq!(
        plan_catch_up(
            &saved,
            current(uuid, BUILD, 900),
            &[plain(&["a"]), must_scan(&["b"])],
        ),
        ReplayOutcome::FullRescan
    );
    for flags in [
        ChangeFlags {
            user_dropped: true,
            ..ChangeFlags::default()
        },
        ChangeFlags {
            kernel_dropped: true,
            ..ChangeFlags::default()
        },
        ChangeFlags {
            event_ids_wrapped: true,
            ..ChangeFlags::default()
        },
        ChangeFlags {
            root_changed: true,
            ..ChangeFlags::default()
        },
    ] {
        let events = [plain(&["a"]), event(&[], flags)];
        assert_eq!(
            plan_catch_up(&saved, current(uuid, BUILD, 900), &events,),
            ReplayOutcome::FullRescan,
            "{flags:?}"
        );
        assert_eq!(plan_batch(&events), ReplayOutcome::FullRescan, "{flags:?}");
    }
}

#[test]
fn a_different_os_build_forces_a_full_rescan() {
    let saved = meta();
    assert_eq!(
        plan_catch_up(&saved, current("0A1B-UUID", "25B456", 500), &[],),
        ReplayOutcome::FullRescan
    );
    assert_eq!(
        plan_catch_up(&saved, current("0A1B-UUID", "", 900), &[plain(&["a"])],),
        ReplayOutcome::FullRescan
    );
}

#[test]
fn live_must_scan_sub_dirs_rescans_only_that_subtree() {
    let events = [
        plain(&["a", "x"]),
        must_scan(&["a"]),
        must_scan(&["a", "deep"]),
        plain(&["b"]),
        must_scan(&["c"]),
        plain(&["b"]),
    ];
    assert_eq!(
        plan_batch(&events),
        ReplayOutcome::Apply {
            reread: vec![path_of(&["b"])],
            rescan: vec![path_of(&["a"]), path_of(&["c"])],
        }
    );
    assert_eq!(plan_batch(&[]), ReplayOutcome::UpToDate);
}
