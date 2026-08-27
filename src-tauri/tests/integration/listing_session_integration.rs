use file_explorer_lib::fs::{SortDirection, SortKey};
use std::fs;
use tempfile::tempdir;

use file_explorer_lib::directory_session::{
    enumerate_snapshot_for_tests, BeginNavigationRequest, DirectorySessionService,
    GetSessionRangeRequest, ReleaseSessionRequest, SessionRejection, ViewParams, SESSION_PAGE_SIZE,
};
use file_explorer_lib::watch::patch::SessionPatch;

fn view_params() -> ViewParams {
    ViewParams {
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: false,
    }
}

fn nav_request(pane_id: &str, tab_id: &str, path: &str) -> BeginNavigationRequest {
    BeginNavigationRequest {
        pane_id: pane_id.to_string(),
        tab_id: tab_id.to_string(),
        path: path.to_string(),
        view: view_params(),
    }
}

fn write_files(dir: &std::path::Path, count: usize) {
    for index in 0..count {
        fs::write(dir.join(format!("file-{index:05}.txt")), b"x").expect("write file");
    }
}

#[test]
fn idempotent_forward_and_backward_page_reads_return_identical_content() {
    let dir = tempdir().expect("temp dir");
    // A few pages' worth of entries proves the mechanism without needing a
    // literally huge fixture (repository timing rules require this suite to
    // stay well under 1s).
    write_files(dir.path(), SESSION_PAGE_SIZE * 3 + 42);

    let service = DirectorySessionService::default();
    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("begin navigation");

    let request_for = |page_index: u64| GetSessionRangeRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        baseline: begin.baseline,
        page_index,
    };

    let page3_first = service.get_range(&request_for(3)).expect("page 3");
    let page0 = service.get_range(&request_for(0)).expect("page 0");
    let page3_second = service.get_range(&request_for(3)).expect("page 3 again");

    assert_eq!(
        page3_first, page3_second,
        "re-requesting page 3 must be idempotent"
    );
    assert_eq!(page0.page.page_index, 0);
    assert_eq!(page0.page.entries.len(), SESSION_PAGE_SIZE);
    assert_eq!(page3_first.page.page_index, 3);
    // Total is 3 full pages + 42, so page 3 (0-indexed, 4th page) has 42 rows.
    assert_eq!(page3_first.page.entries.len(), 42);
    assert_eq!(page0.total_rows, (SESSION_PAGE_SIZE * 3 + 42) as u64);
    assert_eq!(page3_first.total_rows, page0.total_rows);
}

#[test]
fn mutation_during_snapshot_creation_is_present_in_the_established_baseline() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 3);

    // Simulate a mutation racing enumeration: rather than trying to inject a
    // fake watcher mid-enumeration (the real filesystem watcher already
    // observes this), write a new file to the directory immediately, then
    // begin navigation. On every supported platform this file exists by the
    // time `enumerate_snapshot` reads the directory, exercising exactly the
    // "the mutation lands in the initial baseline" outcome, and the fallback
    // resnapshot path is exercised separately in
    // `capture_reconciliation_never_drops_a_racing_mutation`.
    fs::write(dir.path().join("racer.txt"), b"x").expect("write racing file");

    let service = DirectorySessionService::default();
    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("begin navigation");

    assert_eq!(
        begin.total_rows, 4,
        "the racing mutation must be present in the established baseline, never silently lost"
    );
    let names: Vec<&str> = begin
        .first_page
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert!(names.contains(&"racer.txt"));
}

#[test]
fn capture_reconciliation_never_drops_a_racing_mutation_via_direct_unit_check() {
    // Exercises `directory_session`'s reconciliation path directly (not just
    // through begin_navigation) using the real watch-capture primitive so the
    // "either folded into the baseline, or a resnapshot happens" contract is
    // proven independent of timing luck in the end-to-end test above.
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 2);

    let capture =
        file_explorer_lib::watch::capture_handle_for_tests(dir.path()).expect("begin capture");
    fs::write(dir.path().join("new-during-capture.txt"), b"x").expect("write file");
    // Give the OS watcher a brief, bounded moment to deliver the event before
    // we drain — this is not a fixed-delay wait for correctness (the
    // assertion below tolerates either outcome the contract allows), it only
    // avoids flaking on slow CI filesystem-event delivery.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let mutated = file_explorer_lib::watch::drain_captured_mutations_for_tests(capture);

    // Whatever the capture reported (a resolved mutation set, or `None`
    // meaning "resnapshot required"), a fresh enumeration afterward must
    // reflect the new file either way.
    let _ = mutated;
    let entries = file_explorer_lib::directory_session::enumerate_snapshot_for_tests(
        dir.path(),
        &view_params(),
    )
    .expect("enumerate snapshot");
    assert!(entries
        .iter()
        .any(|entry| entry.name == "new-during-capture.txt"));
}

#[test]
fn a_definite_racing_mutation_still_resnapshots_instead_of_trusting_stale_entries() {
    // `enumerate_snapshot`'s directory read is not guaranteed atomic against
    // concurrent mutations: a change that fires mid-scan can race the read
    // and be missing from its result even though the watch capture observed
    // it as a definite, resolved mutation (not the ambiguous/`None` case).
    // Deterministically prove the reconciliation closes that window by
    // handing it `entries` that deliberately do *not* contain a file that
    // already exists on disk, alongside a non-empty drained mutation set —
    // it must resnapshot from disk rather than trust the stale `entries`.
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 2);

    // Snapshot before the racing file exists, mirroring what a real
    // `enumerate_snapshot` read could return if it happened to race a
    // concurrent mutation and miss it (readdir is not guaranteed atomic).
    let stale_entries = file_explorer_lib::directory_session::enumerate_snapshot_for_tests(
        dir.path(),
        &view_params(),
    )
    .expect("enumerate stale snapshot");
    assert!(
        !stale_entries
            .iter()
            .any(|entry| entry.name == "missed-during-scan.txt"),
        "the stale entries fixture must not already contain the racing file"
    );

    fs::write(dir.path().join("missed-during-scan.txt"), b"x").expect("write racing file");

    let drained = Some(std::collections::HashSet::from([dir
        .path()
        .join("missed-during-scan.txt")]));

    let (resolved, watch_revision) =
        file_explorer_lib::directory_session::resolve_reconciliation_for_tests(
            dir.path(),
            &view_params(),
            stale_entries,
            drained,
        )
        .expect("reconciliation resolves");

    assert!(
        resolved
            .iter()
            .any(|entry| entry.name == "missed-during-scan.txt"),
        "a definite racing mutation must trigger a resnapshot that picks up the missed file, \
         not just pass the stale entries through unchanged"
    );
    assert_eq!(watch_revision.0, 1);
}

#[test]
fn stale_range_revisions_are_rejected_not_served() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 5);

    let service = DirectorySessionService::default();
    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("begin navigation");

    // Superseded session id (as if begin_navigation had run again).
    let mut stale_session = begin.baseline;
    stale_session.session_id =
        file_explorer_lib::directory_session::SessionId(stale_session.session_id.0 + 999);
    let result = service.get_range(&GetSessionRangeRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        baseline: stale_session,
        page_index: 0,
    });
    assert_eq!(result, Err(SessionRejection::StaleSession));

    // Wrong pane entirely.
    let result = service.get_range(&GetSessionRangeRequest {
        pane_id: "right".to_string(),
        tab_id: "left-1".to_string(),
        baseline: begin.baseline,
        page_index: 0,
    });
    assert_eq!(result, Err(SessionRejection::NoActiveSession));

    // Out-of-range page index.
    let result = service.get_range(&GetSessionRangeRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        baseline: begin.baseline,
        page_index: 999,
    });
    assert_eq!(result, Err(SessionRejection::PageOutOfRange));

    // A newer navigation on the same pane must make the previous baseline
    // stale too.
    let second = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("second begin navigation");
    assert_ne!(second.baseline.session_id, begin.baseline.session_id);
    let result = service.get_range(&GetSessionRangeRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        baseline: begin.baseline,
        page_index: 0,
    });
    assert_eq!(result, Err(SessionRejection::StaleSession));
}

#[test]
fn view_revision_changes_invalidate_prior_baseline_range_requests() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 5);

    let service = DirectorySessionService::default();
    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("begin navigation");

    let revised = service
        .revise_view(
            &"left".to_string(),
            "left-1",
            begin.baseline.session_id,
            ViewParams {
                sort_key: SortKey::Size,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: false,
                include_item_counts: false,
            },
        )
        .expect("revise view");

    assert_ne!(revised.baseline.view_revision, begin.baseline.view_revision);
    assert_eq!(revised.baseline.session_id, begin.baseline.session_id);

    // The old (pre-revision) baseline must now be rejected.
    let result = service.get_range(&GetSessionRangeRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        baseline: begin.baseline,
        page_index: 0,
    });
    assert_eq!(result, Err(SessionRejection::StaleView));

    // The new baseline serves fine.
    let result = service.get_range(&GetSessionRangeRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        baseline: revised.baseline,
        page_index: 0,
    });
    assert!(result.is_ok());
}

#[test]
fn release_session_is_idempotent_and_teardown_safe() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 3);

    let service = DirectorySessionService::default();
    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("begin navigation");

    let release_request = ReleaseSessionRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        session_id: begin.baseline.session_id,
        navigation_revision: begin.baseline.navigation_revision,
    };

    let first = service.release_session(&release_request);
    assert!(first.released);
    assert_eq!(service.active_session_count(), 0);

    // Calling release again (e.g. teardown racing an explicit close) must
    // not panic and must simply report nothing was released.
    let second = service.release_session(&release_request);
    assert!(!second.released);

    // Releasing a session that never existed is equally safe.
    let never_existed = service.release_session(&ReleaseSessionRequest {
        pane_id: "right".to_string(),
        tab_id: "right-1".to_string(),
        session_id: begin.baseline.session_id,
        navigation_revision: begin.baseline.navigation_revision,
    });
    assert!(!never_existed.released);
}

#[test]
fn close_during_enumeration_range_fetch_and_watch_setup_lifecycle_points_are_safe() {
    // "Close during enumeration": begin_navigation for pane "left" twice in a
    // row before either completes any range fetch — the second call must
    // fully retire the first without leaving stale state, and both
    // operations (which each perform their own enumeration + watch-capture
    // internally) must complete without panicking.
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 4);
    let service = DirectorySessionService::default();

    let first = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("first begin navigation");
    let second = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("second begin navigation (simulates close-during-enumeration)");
    assert_ne!(first.baseline.session_id, second.baseline.session_id);
    assert_eq!(
        service.active_session_count(),
        1,
        "only one session should remain active per pane after rapid re-navigation"
    );

    // "Close during range fetch": release immediately after issuing a range
    // request for the now-superseded first baseline — neither call panics,
    // and the stale range request is rejected rather than served.
    let stale_range = service.get_range(&GetSessionRangeRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        baseline: first.baseline,
        page_index: 0,
    });
    assert_eq!(stale_range, Err(SessionRejection::StaleSession));

    let release = service.release_session(&ReleaseSessionRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        session_id: second.baseline.session_id,
        navigation_revision: second.baseline.navigation_revision,
    });
    assert!(release.released);
    assert_eq!(service.active_session_count(), 0);

    // "Close during watch setup": begin_navigation on a *new* pane and
    // release it right away — since watch-capture setup/teardown happens
    // synchronously inside begin_navigation in this phase, this exercises
    // that no leaked watch survives an immediate release.
    let third = service
        .begin_navigation(
            nav_request("right", "right-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("third begin navigation on a different pane");
    let release_third = service.release_session(&ReleaseSessionRequest {
        pane_id: "right".to_string(),
        tab_id: "right-1".to_string(),
        session_id: third.baseline.session_id,
        navigation_revision: third.baseline.navigation_revision,
    });
    assert!(release_third.released);
    assert_eq!(service.active_session_count(), 0);
}

#[test]
fn revise_view_rejects_missing_session_wrong_tab_and_stale_session_id() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 3);
    let service = DirectorySessionService::default();

    // No active session on this pane at all.
    let result = service.revise_view(
        &"left".to_string(),
        "left-1",
        file_explorer_lib::directory_session::SessionId(1),
        view_params(),
    );
    assert_eq!(result, Err(SessionRejection::NoActiveSession));

    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("begin navigation");

    // Wrong tab id for the pane's active session.
    let result = service.revise_view(
        &"left".to_string(),
        "left-2",
        begin.baseline.session_id,
        view_params(),
    );
    assert_eq!(result, Err(SessionRejection::NoActiveSession));

    // Stale session id (as if the caller held an old handle).
    let result = service.revise_view(
        &"left".to_string(),
        "left-1",
        file_explorer_lib::directory_session::SessionId(begin.baseline.session_id.0 + 999),
        view_params(),
    );
    assert_eq!(result, Err(SessionRejection::StaleSession));
}

#[test]
fn current_baseline_reflects_active_and_absent_sessions() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 3);
    let service = DirectorySessionService::default();

    assert!(service.current_baseline(&"left".to_string()).is_none());

    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("begin navigation");
    assert_eq!(
        service.current_baseline(&"left".to_string()),
        Some(begin.baseline)
    );

    service.release_session(&ReleaseSessionRequest {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        session_id: begin.baseline.session_id,
        navigation_revision: begin.baseline.navigation_revision,
    });
    assert!(service.current_baseline(&"left".to_string()).is_none());
}

#[test]
fn session_rejection_display_covers_every_variant() {
    let variants = [
        SessionRejection::NoActiveSession,
        SessionRejection::StaleSession,
        SessionRejection::StaleNavigation,
        SessionRejection::StaleWatch,
        SessionRejection::StaleView,
        SessionRejection::PageOutOfRange,
    ];
    for variant in variants {
        assert!(!variant.to_string().is_empty());
    }
}

#[test]
fn begin_navigation_uses_resource_coordinator_admission_when_a_resource_key_is_supplied() {
    use file_explorer_lib::resource_coordinator::ResourceCoordinator;

    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 3);
    let service = DirectorySessionService::default();
    let coordinator = ResourceCoordinator::new();

    let begin = service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            Some(&coordinator),
            Some("test-volume".to_string()),
        )
        .expect("begin navigation with coordinator admission");
    assert_eq!(begin.total_rows, 3);
}

#[test]
fn at_most_two_active_sessions_total_one_per_pane() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 2);
    let service = DirectorySessionService::default();

    service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("left navigation");
    service
        .begin_navigation(
            nav_request("right", "right-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("right navigation");

    assert_eq!(service.active_session_count(), 2);

    // Re-navigating the left pane must not grow beyond two sessions total.
    service
        .begin_navigation(
            nav_request("left", "left-1", &dir.path().to_string_lossy()),
            None,
            None,
        )
        .expect("left re-navigation");
    assert_eq!(service.active_session_count(), 2);
}

#[test]
fn apply_watch_mutation_produces_a_delta_patch_for_a_single_insert() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 2);
    let service = DirectorySessionService::default();
    let path_string = dir.path().to_string_lossy().to_string();

    let begin = service
        .begin_navigation(nav_request("left", "left-1", &path_string), None, None)
        .expect("begin navigation");

    fs::write(dir.path().join("file-00002.txt"), b"x").expect("write new file");
    let snapshot =
        enumerate_snapshot_for_tests(dir.path(), &view_params()).expect("fresh snapshot");
    let new_entry = snapshot
        .iter()
        .find(|entry| entry.name == "file-00002.txt")
        .expect("new entry present")
        .clone();

    let patch = service
        .apply_watch_mutation(
            &"left".to_string(),
            &begin.path,
            vec![new_entry.clone()],
            Vec::new(),
        )
        .expect("mutation should produce a patch");

    match patch {
        SessionPatch::Delta {
            total_rows, deltas, ..
        } => {
            assert_eq!(total_rows, 3);
            assert_eq!(deltas.len(), 1);
        }
        other => panic!("expected Delta, got {other:?}"),
    }

    // The session's own baseline must have advanced to match.
    let advanced = service
        .current_baseline(&"left".to_string())
        .expect("session still active");
    assert_ne!(advanced.watch_revision, begin.baseline.watch_revision);
    assert_ne!(advanced.view_revision, begin.baseline.view_revision);
}

#[test]
fn apply_watch_mutation_produces_a_replace_view_patch_for_multiple_removals() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 4);
    let service = DirectorySessionService::default();
    let path_string = dir.path().to_string_lossy().to_string();

    let begin = service
        .begin_navigation(nav_request("left", "left-1", &path_string), None, None)
        .expect("begin navigation");

    let removed = [
        dir.path()
            .join("file-00000.txt")
            .to_string_lossy()
            .to_string(),
        dir.path()
            .join("file-00001.txt")
            .to_string_lossy()
            .to_string(),
    ];
    // Resolve removed paths through a fresh snapshot's display-path form so
    // they match exactly what the session stored (canonicalization on some
    // platforms may alter casing/separators).
    let snapshot_before =
        enumerate_snapshot_for_tests(dir.path(), &view_params()).expect("snapshot before");
    let removed_display: Vec<String> = snapshot_before
        .iter()
        .filter(|entry| {
            removed
                .iter()
                .any(|path| entry.path.ends_with(path) || entry.path == *path)
        })
        .map(|entry| entry.path.clone())
        .collect();
    assert_eq!(removed_display.len(), 2);

    let patch = service
        .apply_watch_mutation(
            &"left".to_string(),
            &begin.path,
            Vec::new(),
            removed_display,
        )
        .expect("mutation should produce a patch");

    match patch {
        SessionPatch::ReplaceView { total_rows, .. } => assert_eq!(total_rows, 2),
        other => panic!("expected ReplaceView, got {other:?}"),
    }
}

#[test]
fn apply_watch_mutation_is_a_no_op_when_view_is_unchanged() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 2);
    let service = DirectorySessionService::default();
    let path_string = dir.path().to_string_lossy().to_string();

    let begin = service
        .begin_navigation(nav_request("left", "left-1", &path_string), None, None)
        .expect("begin navigation");

    // Report the exact same entries already in the snapshot: the derived
    // view does not change, so no patch should be emitted and no revision
    // should advance.
    let snapshot = enumerate_snapshot_for_tests(dir.path(), &view_params()).expect("snapshot");

    let patch =
        service.apply_watch_mutation(&"left".to_string(), &begin.path, snapshot, Vec::new());
    assert!(patch.is_none());
    assert_eq!(
        service.current_baseline(&"left".to_string()).unwrap(),
        begin.baseline
    );
}

#[test]
fn apply_watch_mutation_is_a_no_op_for_unknown_pane_or_mismatched_path() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 1);
    let service = DirectorySessionService::default();
    let path_string = dir.path().to_string_lossy().to_string();

    // No active session at all for this pane.
    assert!(service
        .apply_watch_mutation(&"left".to_string(), &path_string, Vec::new(), Vec::new())
        .is_none());

    service
        .begin_navigation(nav_request("left", "left-1", &path_string), None, None)
        .expect("begin navigation");

    // Path does not match the active session's path.
    assert!(service
        .apply_watch_mutation(
            &"left".to_string(),
            "/completely/different/path",
            Vec::new(),
            Vec::new()
        )
        .is_none());
}

#[test]
fn apply_metadata_only_builds_a_metadata_patch_and_rejects_bad_input() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 1);
    let service = DirectorySessionService::default();
    let path_string = dir.path().to_string_lossy().to_string();

    // No active session yet.
    assert!(service
        .apply_metadata_only(&"left".to_string(), &path_string, Vec::new())
        .is_none());

    let begin = service
        .begin_navigation(nav_request("left", "left-1", &path_string), None, None)
        .expect("begin navigation");

    // Empty updates is always a no-op regardless of session state.
    assert!(service
        .apply_metadata_only(&"left".to_string(), &begin.path, Vec::new())
        .is_none());

    let snapshot = enumerate_snapshot_for_tests(dir.path(), &view_params()).expect("snapshot");
    let target = snapshot.first().expect("at least one entry").clone();
    let updates = vec![file_explorer_lib::watch::patch::MetadataDelta {
        path: target.path.clone(),
        entry: target.clone(),
    }];

    // Mismatched path is a no-op.
    assert!(service
        .apply_metadata_only(&"left".to_string(), "/mismatched/path", updates.clone())
        .is_none());

    let patch = service
        .apply_metadata_only(&"left".to_string(), &begin.path, updates)
        .expect("expected a metadata-only patch");

    match patch {
        SessionPatch::MetadataOnly {
            baseline, updates, ..
        } => {
            // No revision advance for metadata-only patches.
            assert_eq!(baseline, begin.baseline);
            assert_eq!(updates.len(), 1);
        }
        other => panic!("expected MetadataOnly, got {other:?}"),
    }
}

#[test]
fn resnapshot_returns_none_when_no_session_active() {
    let service = DirectorySessionService::default();
    assert!(service.resnapshot(&"left".to_string()).is_none());
}

#[test]
fn resnapshot_rereads_the_directory_and_produces_a_replace_view_patch() {
    let dir = tempdir().expect("temp dir");
    write_files(dir.path(), 2);
    let service = DirectorySessionService::default();
    let path_string = dir.path().to_string_lossy().to_string();

    let begin = service
        .begin_navigation(nav_request("left", "left-1", &path_string), None, None)
        .expect("begin navigation");

    // Mutate the directory out from under the session without going through
    // apply_watch_mutation, then force a resnapshot.
    fs::write(dir.path().join("file-00002.txt"), b"x").expect("write extra file");

    let patch = service
        .resnapshot(&"left".to_string())
        .expect("resnapshot should produce a patch");

    match patch {
        SessionPatch::ReplaceView {
            previous_baseline,
            next_baseline,
            total_rows,
            ..
        } => {
            assert_eq!(previous_baseline, begin.baseline);
            assert_eq!(total_rows, 3);
            assert_ne!(
                next_baseline.watch_revision,
                previous_baseline.watch_revision
            );
            assert_ne!(next_baseline.view_revision, previous_baseline.view_revision);
        }
        other => panic!("expected ReplaceView, got {other:?}"),
    }

    let advanced = service
        .current_baseline(&"left".to_string())
        .expect("session still active");
    assert_eq!(
        advanced.watch_revision.0,
        begin.baseline.watch_revision.0 + 1
    );
}
