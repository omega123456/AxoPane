use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use file_explorer_lib::fs::{ListDirOptions, ListDirOutcome, SortDirection, SortKey};
use file_explorer_lib::listing::ListingService;
use file_explorer_lib::watch::WatchTarget;
use tempfile::tempdir;

fn options(path: &str) -> ListDirOptions {
    ListDirOptions {
        path: path.to_string(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: false,
    }
}

fn target(path: &str) -> WatchTarget {
    WatchTarget { tab_id: "left-1".to_string(), path: path.to_string(), sort_key: SortKey::Name, sort_direction: SortDirection::Asc, filter: String::new(), show_hidden: false, include_item_counts: false }
}

#[test]
fn superseded_session_cancels_backend_listing_before_completion() {
    let dir = tempdir().expect("temp dir");
    for index in 0..32 {
        fs::write(dir.path().join(format!("file-{index}.txt")), b"x").expect("write file");
    }

    let service = ListingService::default();
    let first = service.begin_session("left-1");
    let triggered = AtomicBool::new(false);
    let result = file_explorer_lib::fs::list_dir_with_cancellation(
        &options(&dir.path().to_string_lossy()),
        || {
            if !triggered.swap(true, Ordering::SeqCst) {
                service.begin_session("left-1");
            }
            service.is_cancelled(&first)
        },
    )
    .expect("listing outcome");

    assert!(matches!(result, ListDirOutcome::Cancelled));
    assert!(service.completed_for_tab("left-1").is_none());
}

#[test]
fn independent_tabs_keep_separate_active_sessions() {
    let service = ListingService::default();

    let left = service.begin_session("left-1");
    let right = service.begin_session("right-1");
    let newer_left = service.begin_session("left-1");

    assert!(service.is_cancelled(&left));
    assert!(!service.is_cancelled(&right));
    assert!(!service.is_cancelled(&newer_left));
}

#[test]
fn completed_listing_is_retained_for_the_current_tab_session() {
    let dir = tempdir().expect("temp dir");
    fs::write(dir.path().join("beta.txt"), b"x").expect("write file");
    fs::write(dir.path().join("alpha.txt"), b"x").expect("write file");

    let service = ListingService::default();
    let session = service.begin_session("left-1");
    let outcome = file_explorer_lib::fs::list_dir_with_cancellation(
        &options(&dir.path().to_string_lossy()),
        || service.is_cancelled(&session),
    )
    .expect("listing outcome");

    let ListDirOutcome::Complete(response) = outcome else {
        panic!("expected completed listing");
    };
    let options = options(&response.path);
    assert!(service.complete_session(&session, response.path.clone(), options.sort_key, options.sort_direction, options.filter, options.show_hidden, options.include_item_counts, response.entries.clone()));

    let completed = service
        .completed_for_tab("left-1")
        .expect("completed listing snapshot");
    assert_eq!(completed.request_id, session.request_id);
    assert_eq!(completed.path, response.path);
    assert_eq!(
        completed
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha.txt", "beta.txt"]
    );
}

#[test]
fn completed_seed_requires_exact_tab_and_explicit_platform_path_policy() {
    let service = ListingService::default();
    let session = service.begin_session("left-1");
    assert!(service.complete_session(&session, "C:\\AxoPane".to_string(), SortKey::Name, SortDirection::Asc, String::new(), false, false, Vec::new()));

    assert!(service
        .completed_seed_entries("left-1", session.request_id, &target("C:\\AxoPane"))
        .is_some());
    assert!(service
        .completed_seed_entries("LEFT-1", session.request_id, &target("C:\\AxoPane"))
        .is_none());

    let case_variant = "c:\\axopane";
    let found = service.completed_seed_entries("left-1", session.request_id, &target(case_variant));
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        assert!(found.is_some());
    } else {
        assert!(found.is_none());
    }
}

#[test]
fn completed_seed_requires_full_listing_target_context() {
    let service = ListingService::default();
    let session = service.begin_session("left-1");
    assert!(service.complete_session(&session, "C:\\AxoPane".to_string(), SortKey::Name, SortDirection::Asc, "draft".to_string(), true, false, Vec::new()));
    let mut matching = target("C:\\AxoPane");
    matching.filter = "draft".to_string();
    matching.show_hidden = true;
    assert!(service.completed_seed_entries("left-1", session.request_id, &matching).is_some());
    matching.sort_key = SortKey::Size;
    assert!(service.completed_seed_entries("left-1", session.request_id, &matching).is_none());
    matching.sort_key = SortKey::Name;
    matching.filter = "other".to_string();
    assert!(service.completed_seed_entries("left-1", session.request_id, &matching).is_none());
    matching.filter = "draft".to_string();
    matching.show_hidden = false;
    assert!(service.completed_seed_entries("left-1", session.request_id, &matching).is_none());
    matching.show_hidden = true;
    matching.include_item_counts = true;
    assert!(service.completed_seed_entries("left-1", session.request_id, &matching).is_none());
}
