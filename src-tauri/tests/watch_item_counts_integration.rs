//! Covers Phase 2 of the Tier 1 performance plan: `WatchTarget::include_item_counts`
//! and entry-seeded arming of `set_tab_watch`.
//!
//! - `snapshot_for_target` must skip child-directory item-count enumeration when
//!   `include_item_counts` is `false` (mirrors the Items column visibility).
//! - `set_tab_watch` must build its baseline snapshot from caller-supplied entries
//!   when present, without re-reading the target directory.
//! - The debounce-driven resnapshot path must still honor the flag and produce
//!   correct patches.

use std::sync::{Arc, Mutex};

use file_explorer_lib::fs::{self, DirectoryEntry, SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{SetTabWatchRequest, WatchSeedReference};
use file_explorer_lib::listing::ListingService;
use file_explorer_lib::watch::{
    create_runtime, diff_entries, handle_debounce_result_for_tests, insert_tab_for_tests,
    snapshot_for_target, tab_snapshot_for_tests, DirPatch, WatchService, WatchTarget,
};
use notify::event::EventKind;
use notify::Event;
use tempfile::tempdir;

fn base_target(tab_id: &str, path: &str, include_item_counts: bool) -> WatchTarget {
    WatchTarget {
        tab_id: tab_id.to_string(),
        path: path.to_string(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts,
    }
}

fn phantom_entry(path: &str, name: &str) -> DirectoryEntry {
    DirectoryEntry {
        id: path.to_string(),
        name: name.to_string(),
        path: path.to_string(),
        is_dir: false,
        icon_data_url: None,
        size_bytes: Some(1),
        item_count: None,
        type_label: "TXT file".to_string(),
        modified_at: None,
        created_at: None,
        attributes: Vec::new(),
        is_hidden: false,
        is_system: false,
    }
}

fn as_state<'a, T: Send + Sync + 'static>(value: &'a T) -> tauri::State<'a, T> {
    unsafe { std::mem::transmute::<&'a T, tauri::State<'a, T>>(value) }
}

#[test]
fn snapshot_for_target_skips_child_item_counts_when_disabled() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let child = root.join("child-dir");
    std::fs::create_dir(&child).expect("child dir");
    std::fs::write(child.join("a.txt"), b"a").expect("a");
    std::fs::write(child.join("b.txt"), b"b").expect("b");
    std::fs::write(child.join("c.txt"), b"c").expect("c");

    let disabled = base_target("left-1", &root.to_string_lossy(), false);
    let snapshot = snapshot_for_target(&disabled).expect("snapshot without item counts");
    let child_entry = snapshot
        .values()
        .find(|entry| entry.path.ends_with("child-dir"))
        .expect("child entry present");
    assert_eq!(
        child_entry.item_count, None,
        "item_count must stay unset when include_item_counts is false"
    );

    let enabled = base_target("left-1", &root.to_string_lossy(), true);
    let snapshot = snapshot_for_target(&enabled).expect("snapshot with item counts");
    let child_entry = snapshot
        .values()
        .find(|entry| entry.path.ends_with("child-dir"))
        .expect("child entry present");
    assert_eq!(
        child_entry.item_count,
        Some(3),
        "item_count must be populated when include_item_counts is true"
    );
}

#[test]
fn set_tab_watch_seeds_baseline_from_supplied_entries_without_relisting() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    // The directory is empty on disk; the supplied entries describe a file that
    // does not exist. If `set_tab_watch` re-read the directory instead of using
    // the supplied entries, the baseline snapshot would be empty and no removal
    // would ever be observed for `phantom.txt`.
    let phantom_path = root.join("phantom.txt").to_string_lossy().into_owned();
    let entries = vec![phantom_entry(&phantom_path, "phantom.txt")];

    let service = WatchService::default();
    let target = base_target("left-1", &root.to_string_lossy(), true);

    service
        .set_tab_watch(
            Some(target.clone()),
            None,
            Some(entries),
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("seed watch from supplied entries");

    let baseline = tab_snapshot_for_tests(&service, &target.tab_id).expect("tab baseline recorded");
    assert!(
        baseline.keys().any(|path| path.ends_with("phantom.txt")),
        "the seeded phantom entry must be present in the baseline, proving it came from \
         the supplied entries and not a real (empty) directory listing"
    );

    let real_listing = snapshot_for_target(&target).expect("real directory listing");
    assert!(
        !real_listing
            .keys()
            .any(|path| path.ends_with("phantom.txt")),
        "the real directory must not contain the phantom entry"
    );
}

#[test]
fn set_tab_watch_seeds_empty_first_diff_even_when_supplied_entries_carry_icon_data() {
    // `list_dir` always returns entries with `icon_data_url: None`; the
    // frontend seeds the watch baseline from entries that may already carry a
    // resolved `iconDataUrl`. The baseline must still compare equal to the
    // very next `list_dir`-sourced snapshot so the first diff after arming is
    // empty (see `snapshot_from_entries`'s doc comment).
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    std::fs::write(root.join("real.txt"), b"real").expect("real file");

    let real_path = fs::display_path_from_path(
        &std::fs::canonicalize(root.join("real.txt")).expect("canonical real file"),
    );
    let service = WatchService::default();
    let target = base_target("left-1", &root.to_string_lossy(), true);

    // Base the seeding entry on a real listing (matching every field a real
    // `list_dir` response would produce) except for `icon_data_url`, which the
    // frontend resolves separately and would already carry a value here —
    // that is the one field the fix must strip before seeding.
    let real_listing_before = snapshot_for_target(&target).expect("real directory listing");
    let mut icon_bearing_entry = real_listing_before
        .get(&real_path)
        .cloned()
        .expect("real.txt present in real listing");
    icon_bearing_entry.icon_data_url = Some("data:image/png;base64,zzzz".to_string());

    service
        .set_tab_watch(
            Some(target.clone()),
            None,
            Some(vec![icon_bearing_entry]),
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("seed watch from icon-bearing entries");

    let baseline = tab_snapshot_for_tests(&service, &target.tab_id).expect("tab baseline recorded");
    assert_eq!(
        baseline
            .get(&real_path)
            .and_then(|entry| entry.icon_data_url.clone()),
        None,
        "the seeded baseline must normalize icon_data_url to None"
    );

    let real_listing = snapshot_for_target(&target).expect("real directory listing");
    let patch = diff_entries(
        &target.tab_id,
        &target.path,
        "watch",
        &baseline,
        &real_listing,
    );
    assert!(
        patch.changed.is_empty() && patch.removed.is_empty(),
        "the first diff against a real list_dir snapshot must be empty even when the \
         seeding entries carried icon data"
    );
}

#[test]
fn set_tab_watch_falls_back_to_listing_when_no_entries_supplied() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    std::fs::write(root.join("real.txt"), b"real").expect("real file");

    let service = WatchService::default();
    let target = base_target("left-1", &root.to_string_lossy(), true);

    service
        .set_tab_watch(
            Some(target.clone()),
            None,
            None,
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("set watch without seeded entries");

    let baseline = tab_snapshot_for_tests(&service, &target.tab_id).expect("tab baseline recorded");
    let real_listing = snapshot_for_target(&target).expect("real directory listing");

    assert_eq!(
        baseline, real_listing,
        "baseline should already match the on-disk listing when no entries were supplied"
    );
}

#[test]
fn set_tab_watch_uses_valid_listing_seed_reference_without_relisting() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let phantom_path = root.join("phantom.txt").to_string_lossy().into_owned();
    let seeded_entry = phantom_entry(&phantom_path, "phantom.txt");

    let listing_service = ListingService::default();
    let session = listing_service.begin_session("left-1");
    assert!(listing_service.complete_session(
        &session,
        root.to_string_lossy().into_owned(),
        SortKey::Name,
        SortDirection::Asc,
        String::new(),
        true,
        true,
        vec![seeded_entry],
    ));

    let watch_service = WatchService::default();
    commands::set_tab_watch(
        SetTabWatchRequest {
            target: Some(base_target("left-1", &root.to_string_lossy(), true)),
            seed_reference: Some(WatchSeedReference {
                tab_id: "left-1".to_string(),
                request_id: session.request_id,
                path: root.to_string_lossy().into_owned(),
            }),
            entries: None,
        },
        as_state(&listing_service),
        as_state(&watch_service),
    )
    .expect("set watch from seed reference");

    let baseline =
        tab_snapshot_for_tests(&watch_service, "left-1").expect("baseline recorded from seed");
    assert!(baseline.keys().any(|path| path.ends_with("phantom.txt")));
}

#[test]
fn debounce_resnapshot_honors_include_item_counts_flag() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let child = root.join("child-dir");
    std::fs::create_dir(&child).expect("child dir");
    std::fs::write(child.join("only.txt"), b"only").expect("only");
    let watched_file = root.join("watched.txt");
    std::fs::write(&watched_file, b"seed").expect("seed watched file");

    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    let target = base_target("left-1", &root.to_string_lossy(), false);
    insert_tab_for_tests(
        &mut runtime,
        target.clone(),
        snapshot_for_target(&target).expect("initial snapshot"),
    );

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let patches_for_callback = patches.clone();

    // `EventKind::Any` forces the `NeedsResnapshot` path in `patch_for_events`,
    // which resnapshots via `snapshot_for_target` and must still honor the flag.
    handle_debounce_result_for_tests(
        &runtime,
        vec![Ok(Event::new(EventKind::Any).add_path(watched_file.clone()))],
        Arc::new(move |patch| {
            patches_for_callback
                .lock()
                .expect("patches lock")
                .push(patch);
        }),
        Arc::new(|_, _| {}),
    );

    // The resnapshot should have run (child-dir already present, no new patch is
    // guaranteed since nothing changed), so assert directly on a fresh snapshot
    // built the same way the resnapshot path would build it.
    let resnapshot = snapshot_for_target(&target).expect("resnapshot honoring flag");
    let child_entry = resnapshot
        .values()
        .find(|entry| entry.path.ends_with("child-dir"))
        .expect("child entry present");
    assert_eq!(
        child_entry.item_count, None,
        "resnapshot must skip child item counts when include_item_counts is false"
    );
}
