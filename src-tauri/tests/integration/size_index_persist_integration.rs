use std::fs;
use std::path::PathBuf;

use file_explorer_lib::size::index::persist::{
    decode, encode, load, plan_batch, plan_catch_up, save, ChangeEvent, ChangeFlags, ReplayOutcome,
    SnapshotMeta, FORMAT_VERSION,
};
use file_explorer_lib::size::index::tree::FolderSizeTree;
use tempfile::tempdir;

fn p(parts: &[&str]) -> PathBuf {
    std::iter::once("idx")
        .chain(parts.iter().copied())
        .collect()
}

const BUILD: &str = "25A123";

fn meta() -> SnapshotMeta {
    SnapshotMeta {
        volume_uuid: "0A1B-UUID".into(),
        os_build: BUILD.into(),
        event_id: 500,
        last_full_scan_secs: 1_760_000_000,
        visited_roots: vec![p(&["home", "Desktop"])],
    }
}

fn sample_tree() -> FolderSizeTree {
    let mut tree = FolderSizeTree::default();
    tree.add_root(&p(&[]));
    tree.add_root(&["second"].iter().collect::<PathBuf>());
    tree.merge_record(&p(&[]), 1, &["home", "pending"], false);
    tree.merge_record(
        &p(&["home"]),
        20,
        &["zeta", "alpha", "locked", "Desktop"],
        false,
    );
    tree.merge_record::<&str>(&p(&["home", "zeta"]), 300, &[], false);
    tree.merge_record::<&str>(&p(&["home", "alpha"]), 4000, &[], false);
    tree.merge_record::<&str>(&p(&["home", "locked"]), 0, &[], true);
    tree.set_skipped(&p(&["home", "Desktop"]), true);
    tree.merge_record::<&str>(&p(&["pending", "child"]), 7, &[], false);
    tree.merge_record::<&str>(&["second", "x"].iter().collect::<PathBuf>(), 9, &[], false);
    tree
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn put_str(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

/// A checksummed file with an empty header and the given node bytes, so a
/// test can isolate one defect at a time.
fn crafted(version: u32, visited: u32, node_count: u32, nodes: &[u8]) -> Vec<u8> {
    let mut out = b"AXOSIZE\0".to_vec();
    out.extend_from_slice(&version.to_le_bytes());
    put_str(&mut out, b"uuid");
    put_str(&mut out, b"build");
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&visited.to_le_bytes());
    out.extend_from_slice(&node_count.to_le_bytes());
    out.extend_from_slice(nodes);
    let checksum = fnv1a(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    out
}

fn node(parent: u32, name: &[u8], flags: u8) -> Vec<u8> {
    let mut out = parent.to_le_bytes().to_vec();
    put_str(&mut out, name);
    out.extend_from_slice(&5_u64.to_le_bytes());
    out.push(flags);
    out
}

#[test]
fn round_trip_is_byte_exact_and_restores_totals() {
    let tree = sample_tree();
    let bytes = encode(&meta(), &tree);
    let snapshot = decode(&bytes).expect("decodes");
    assert_eq!(snapshot.meta, meta());
    assert_eq!(encode(&snapshot.meta, &snapshot.tree), bytes);
    assert!(!snapshot.tree.is_dirty());
    assert_eq!(snapshot.tree.node_count(), tree.node_count());
    for parts in [
        &[][..],
        &["home"][..],
        &["home", "alpha"][..],
        &["home", "locked"][..],
        &["home", "Desktop"][..],
        &["pending"][..],
    ] {
        assert_eq!(
            snapshot.tree.total(&p(parts)),
            tree.total(&p(parts)),
            "{parts:?}"
        );
    }
    let second: PathBuf = ["second"].iter().collect();
    assert_eq!(snapshot.tree.total(&second).map(|t| t.bytes), Some(9));
    let home = snapshot.tree.total(&p(&["home"])).expect("home");
    assert_eq!(home.bytes, 4320);
    assert_eq!(home.lower_bound.expect("bound").denied_count, 1);
    // Pending placeholders survive, so an interrupted scan can resume.
    assert_eq!(snapshot.tree.unscanned_tops(), tree.unscanned_tops(),);
    assert!(!snapshot.tree.unscanned_tops().is_empty());
}

#[test]
fn save_is_atomic_and_load_reads_it_back() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("cache").join("size-index.bin");
    save(&path, &SnapshotMeta::default(), &FolderSizeTree::default()).expect("first save");
    save(&path, &meta(), &sample_tree()).expect("overwrite");
    assert_eq!(
        fs::read(&path).expect("written"),
        encode(&meta(), &sample_tree())
    );
    assert!(!path.with_extension("tmp").exists());
    assert_eq!(load(&path).expect("loads").meta, meta());

    assert!(load(&dir.path().join("missing.bin")).is_none());
    let blocked = dir.path().join("file");
    fs::write(&blocked, b"x").expect("file");
    assert!(save(&blocked.join("snap.bin"), &meta(), &sample_tree()).is_err());
}

#[test]
fn any_corrupted_byte_or_truncation_yields_none() {
    let bytes = encode(&meta(), &sample_tree());
    for at in 0..bytes.len() {
        let mut corrupt = bytes.clone();
        corrupt[at] ^= 0x41;
        assert!(decode(&corrupt).is_none(), "flip at {at}");
    }
    for len in 0..bytes.len() {
        assert!(decode(&bytes[..len]).is_none(), "truncated to {len}");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(decode(&longer).is_none());
}

#[test]
fn old_version_and_malformed_content_yield_none_even_with_a_valid_checksum() {
    let root = node(u32::MAX, b"idx", 1);
    assert!(
        decode(&crafted(FORMAT_VERSION, 0, 1, &root)).is_some(),
        "control"
    );

    assert!(decode(&crafted(FORMAT_VERSION - 1, 0, 1, &root)).is_none());
    assert!(decode(&crafted(FORMAT_VERSION + 1, 0, 1, &root)).is_none());
    assert!(
        decode(&crafted(FORMAT_VERSION, 0, 0, &root)).is_none(),
        "trailing"
    );
    assert!(decode(&crafted(FORMAT_VERSION, 2_000, 0, &[])).is_none());
    assert!(decode(&crafted(FORMAT_VERSION, 1, 0, &[0xFF; 4])).is_none());
    assert!(decode(&crafted(FORMAT_VERSION, 0, u32::MAX, &root)).is_none());

    let mut huge_name = u32::MAX.to_le_bytes().to_vec();
    huge_name.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(&crafted(FORMAT_VERSION, 0, 1, &huge_name)).is_none());

    for bad in [
        node(u32::MAX, b"", 1),
        node(u32::MAX, b"idx", 0x80),
        node(u32::MAX, &[0xC3, 0x28], 1),
        node(7, b"orphan", 1),
    ] {
        assert!(decode(&crafted(FORMAT_VERSION, 0, 1, &bad)).is_none());
    }
    let duplicate = [root.clone(), node(0, b"a", 1), node(0, b"a", 1)].concat();
    assert!(decode(&crafted(FORMAT_VERSION, 0, 3, &duplicate)).is_none());
}

fn event(parts: &[&str], flags: ChangeFlags) -> ChangeEvent {
    ChangeEvent {
        path: p(parts),
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
        plan_catch_up(&meta(), "0A1B-UUID", BUILD, 500, &[]),
        ReplayOutcome::UpToDate
    );
}

#[test]
fn catch_up_advanced_id_rereads_reported_directories() {
    let events = [plain(&["b"]), plain(&["a"]), plain(&["b"])];
    assert_eq!(
        plan_catch_up(&meta(), "0A1B-UUID", BUILD, 900, &events),
        ReplayOutcome::Apply {
            reread: vec![p(&["a"]), p(&["b"])],
            rescan: vec![],
        }
    );
    assert_eq!(
        plan_catch_up(&meta(), "0A1B-UUID", BUILD, 900, &[]),
        ReplayOutcome::UpToDate
    );
}

#[test]
fn catch_up_falls_back_to_a_full_rescan() {
    let saved = meta();
    let uuid = "0A1B-UUID";
    assert_eq!(
        plan_catch_up(&saved, "OTHER", BUILD, 900, &[]),
        ReplayOutcome::FullRescan
    );
    assert_eq!(
        plan_catch_up(&saved, uuid, BUILD, 499, &[]),
        ReplayOutcome::FullRescan
    );
    assert_eq!(
        plan_catch_up(
            &saved,
            uuid,
            BUILD,
            900,
            &[plain(&["a"]), must_scan(&["b"])]
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
            plan_catch_up(&saved, uuid, BUILD, 900, &events),
            ReplayOutcome::FullRescan,
            "{flags:?}"
        );
        assert_eq!(plan_batch(&events), ReplayOutcome::FullRescan, "{flags:?}");
    }
}

#[test]
fn a_different_os_build_forces_a_full_rescan_and_round_trips() {
    let saved = meta();
    assert_eq!(
        plan_catch_up(&saved, "0A1B-UUID", "25B456", 500, &[]),
        ReplayOutcome::FullRescan
    );
    assert_eq!(
        plan_catch_up(&saved, "0A1B-UUID", "", 900, &[plain(&["a"])]),
        ReplayOutcome::FullRescan
    );
    let decoded = decode(&encode(&saved, &sample_tree())).expect("decodes");
    assert_eq!(decoded.meta.os_build, BUILD);
    // A snapshot from before the build was stored is a different version.
    assert_eq!(FORMAT_VERSION, 2);
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
            reread: vec![p(&["b"])],
            rescan: vec![p(&["a"]), p(&["c"])],
        }
    );
    assert_eq!(plan_batch(&[]), ReplayOutcome::UpToDate);
}
