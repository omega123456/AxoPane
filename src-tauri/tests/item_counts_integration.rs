use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use file_explorer_lib::directory_session::{
    BeginNavigationRequest, DirectorySessionService, ViewParams,
};
use file_explorer_lib::fs::{self as explorer_fs, SortDirection, SortKey};
use file_explorer_lib::item_counts::cache::{
    ItemCountCache, ItemCountState, ITEM_COUNT_CACHE_LIMIT,
};
use file_explorer_lib::item_counts::{
    ActiveItemsSortRequest, ActiveItemsSortResponse, ItemCountRequestContext, ItemCountService,
    VisibleItemCountsRequest, AUTO_ITEM_COUNT_LIMIT, MAX_AUTOMATIC_ITEM_COUNT_QUEUE,
};
use tempfile::tempdir;

fn context(path: &str) -> ItemCountRequestContext {
    ItemCountRequestContext {
        pane_id: "left".to_string(),
        tab_id: "left-1".to_string(),
        request_id: 7,
        path: path.to_string(),
    }
}

#[test]
fn automatic_requests_are_bounded_and_batched() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let mut paths = Vec::new();
    for index in 0..(AUTO_ITEM_COUNT_LIMIT + 25) {
        let child = root.join(format!("dir-{index:03}"));
        fs::create_dir(&child).expect("child dir");
        fs::write(child.join("a.txt"), b"a").expect("seed child");
        paths.push(child.to_string_lossy().into_owned());
    }

    let service = ItemCountService::default();
    let request = VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths,
    };
    let plan = service.plan_automatic_request(&request);
    let mut events = Vec::new();
    service.process_automatic_request(plan, |event| events.push(event));

    let total_results = events
        .iter()
        .map(|event| event.results.len())
        .sum::<usize>();
    assert_eq!(total_results, AUTO_ITEM_COUNT_LIMIT);
    assert!(
        events.len() >= 2,
        "large requests should flush multiple batches"
    );
    assert!(events
        .iter()
        .take(events.len() - 1)
        .all(|event| !event.done));
    assert!(events.last().is_some_and(|event| event.done));
    assert!(events
        .iter()
        .flat_map(|event| event.results.iter())
        .all(|result| result.item_count == Some(1)));
}

#[test]
fn automatic_requests_cancel_only_the_stale_scope() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    for index in 0..80 {
        let child = root.join(format!("dir-{index}"));
        fs::create_dir(&child).expect("child dir");
        fs::write(child.join("seed.txt"), b"x").expect("seed child");
    }

    let service = ItemCountService::default();
    let stale_request = VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: (0..80)
            .map(|index| {
                root.join(format!("dir-{index}"))
                    .to_string_lossy()
                    .into_owned()
            })
            .collect(),
    };
    let stale_plan = service.plan_automatic_request(&stale_request);
    let stale_cancelled = AtomicBool::new(false);
    let mut stale_events = Vec::new();
    service.process_automatic_request(stale_plan, |event| {
        stale_events.push(event);
        if !stale_cancelled.swap(true, Ordering::SeqCst) {
            let newer_request = VisibleItemCountsRequest {
                context: ItemCountRequestContext {
                    request_id: 8,
                    ..context(&root.to_string_lossy())
                },
                paths: vec![root.join("dir-0").to_string_lossy().into_owned()],
            };
            let _ = service.plan_automatic_request(&newer_request);
        }
    });

    assert_eq!(
        stale_events.len(),
        1,
        "stale work should stop after supersession"
    );
    assert!(
        !stale_events[0].done,
        "cancelled work must not emit a terminal done batch"
    );

    let other_request = VisibleItemCountsRequest {
        context: ItemCountRequestContext {
            pane_id: "right".to_string(),
            tab_id: "right-1".to_string(),
            request_id: 1,
            path: root.to_string_lossy().into_owned(),
        },
        paths: vec![root.join("dir-1").to_string_lossy().into_owned()],
    };
    let other_plan = service.plan_automatic_request(&other_request);
    let mut other_events = Vec::new();
    service.process_automatic_request(other_plan, |event| other_events.push(event));
    assert_eq!(other_events.len(), 1);
    assert!(other_events[0].done);
    assert_eq!(other_events[0].results[0].item_count, Some(1));
}

#[test]
fn cancelled_automatic_paths_are_released_for_a_later_retry() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    for index in 0..65 {
        let child = root.join(format!("dir-{index}"));
        fs::create_dir(&child).expect("child dir");
    }

    let service = ItemCountService::default();
    let paths = (0..65)
        .map(|index| {
            root.join(format!("dir-{index}"))
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    let stale = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: paths.clone(),
    });

    service.process_automatic_request(stale, |_| {
        let _ = service.plan_automatic_request(&VisibleItemCountsRequest {
            context: ItemCountRequestContext {
                request_id: 8,
                ..context(&root.to_string_lossy())
            },
            paths: vec![paths[0].clone()],
        });
    });

    let retry = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: ItemCountRequestContext {
            request_id: 8,
            ..context(&root.to_string_lossy())
        },
        paths: vec![paths[64].clone()],
    });
    assert!(
        !retry.is_empty(),
        "a path left unreported by a cancelled plan must be selectable again"
    );
}

#[test]
fn queued_automatic_work_is_coalesced_bounded_and_keeps_latest_paths() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    for name in ["first", "latest"] {
        let child = root.join(name);
        fs::create_dir(&child).expect("child");
        fs::write(child.join("entry"), b"x").expect("entry");
    }
    let service = ItemCountService::default();
    let first = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: vec![root.join("first").to_string_lossy().into_owned()],
    });
    assert!(service.enqueue_automatic_request(first));
    let latest = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: vec![root.join("latest").to_string_lossy().into_owned()],
    });
    let _ = service.enqueue_automatic_request(latest);
    assert_eq!(service.automatic_queue_len(), 1);
    let mut events = Vec::new();
    service.process_automatic_queue(|event| events.push(event));
    let paths = events
        .into_iter()
        .flat_map(|event| event.results)
        .map(|result| result.path)
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("first")));
    assert!(paths.iter().any(|path| path.ends_with("latest")));

    for index in 0..(MAX_AUTOMATIC_ITEM_COUNT_QUEUE + 8) {
        let plan = service.plan_automatic_request(&VisibleItemCountsRequest {
            context: ItemCountRequestContext {
                pane_id: format!("pane-{index}"),
                ..context(&root.to_string_lossy())
            },
            paths: vec![root.join("latest").to_string_lossy().into_owned()],
        });
        let _ = service.enqueue_automatic_request(plan);
    }
    assert!(service.automatic_queue_len() <= MAX_AUTOMATIC_ITEM_COUNT_QUEUE);
}

#[test]
fn unreadable_or_missing_directories_return_unknown_counts() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing-dir");
    let service = ItemCountService::default();
    let request = VisibleItemCountsRequest {
        context: context(&fixture.path().to_string_lossy()),
        paths: vec![missing.to_string_lossy().into_owned()],
    };

    let plan = service.plan_automatic_request(&request);
    let mut events = Vec::new();
    service.process_automatic_request(plan, |event| events.push(event));

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].results.len(), 1);
    assert_eq!(events[0].results[0].item_count, None);
    assert!(events[0].done);
}

#[test]
fn explicit_items_sort_returns_stable_backend_order_and_unknowns() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let alpha = root.join("alpha");
    let beta = root.join("beta");
    let gamma = root.join("gamma");
    fs::create_dir(&alpha).expect("alpha");
    fs::create_dir(&beta).expect("beta");
    fs::create_dir(&gamma).expect("gamma");
    fs::write(alpha.join("1.txt"), b"1").expect("alpha child");
    fs::write(alpha.join("2.txt"), b"2").expect("alpha child");
    fs::write(beta.join("1.txt"), b"1").expect("beta child");

    let missing_path = root.join("missing");
    let service = ItemCountService::default();
    let response = service
        .sort_active_items(&ActiveItemsSortRequest {
            context: context(&root.to_string_lossy()),
            sort_direction: SortDirection::Desc,
            filter: String::new(),
            show_hidden: true,
        })
        .expect("sort items");

    let ActiveItemsSortResponse::Ready(ready) = response else {
        panic!("expected ready result");
    };

    let names = ready
        .entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.item_count))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            ("alpha".to_string(), Some(2)),
            ("beta".to_string(), Some(1)),
            ("gamma".to_string(), Some(0)),
        ]
    );

    let unknown_request = VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: vec![missing_path.to_string_lossy().into_owned()],
    };
    let plan = service.plan_automatic_request(&unknown_request);
    let mut events = Vec::new();
    service.process_automatic_request(plan, |event| events.push(event));
    assert_eq!(events[0].results[0].item_count, None);
}

#[test]
fn explicit_items_sort_serves_full_relisting_and_path_matching_is_case_fallback_safe() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("dir-a")).expect("dir-a");
    fs::create_dir(root.join("dir-b")).expect("dir-b");
    fs::write(root.join("dir-a").join("1.txt"), b"1").expect("seed");
    fs::write(root.join("dir-b").join("1.txt"), b"1").expect("seed");
    fs::write(root.join("dir-b").join("2.txt"), b"2").expect("seed");
    fs::write(root.join("note.txt"), b"note").expect("file");

    let service = ItemCountService::default();
    let mixed_case = root.to_string_lossy().to_string().to_uppercase();
    let first = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: vec![root.join("dir-a").to_string_lossy().into_owned()],
    });
    let second = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: ItemCountRequestContext {
            path: mixed_case,
            ..context(&root.to_string_lossy())
        },
        paths: vec![root.join("dir-a").to_string_lossy().into_owned()],
    });
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        assert!(
            !first.is_empty() && !second.is_empty(),
            "the latest coalesced plan carries the earlier visible path forward"
        );
    } else {
        assert!(
            !first.is_empty() && !second.is_empty(),
            "POSIX paths remain distinct"
        );
    }

    let response = service
        .sort_active_items(&ActiveItemsSortRequest {
            context: ItemCountRequestContext {
                path: root.to_string_lossy().into_owned(),
                ..context(&root.to_string_lossy())
            },
            sort_direction: SortDirection::Asc,
            filter: String::new(),
            show_hidden: true,
        })
        .expect("full relisting sort");

    let ActiveItemsSortResponse::Ready(ready) = response else {
        panic!("expected ready result");
    };
    let canonical_root = file_explorer_lib::fs::canonicalize_dir(root).expect("canonical root");
    assert_eq!(
        ready.path,
        file_explorer_lib::fs::display_path_from_path(&canonical_root)
    );
    assert_eq!(
        ready
            .entries
            .iter()
            .map(|entry| (&entry.name, entry.item_count))
            .collect::<Vec<_>>(),
        vec![
            (&"dir-a".to_string(), Some(1)),
            (&"dir-b".to_string(), Some(2)),
            (&"note.txt".to_string(), None),
        ]
    );
}

#[test]
fn automatic_context_keeps_pane_and_tab_ids_exact() {
    let fixture = tempdir().expect("temp dir");
    let child = fixture.path().join("child");
    fs::create_dir(&child).expect("child dir");
    let service = ItemCountService::default();
    let path = child.to_string_lossy().into_owned();

    let first = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: context(&fixture.path().to_string_lossy()),
        paths: vec![path.clone()],
    });
    let second = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: ItemCountRequestContext {
            pane_id: "Left".to_string(),
            tab_id: "LEFT-1".to_string(),
            ..context(&fixture.path().to_string_lossy())
        },
        paths: vec![path],
    });

    assert!(!first.is_empty());
    assert!(!second.is_empty());
}

#[test]
fn cancel_tab_drops_queued_and_active_work_for_that_tab_only() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("child")).expect("child");
    fs::write(root.join("child").join("a"), b"a").expect("seed");
    let service = ItemCountService::default();

    // Populate the automatic map (active context for tab left-1) and the queue.
    let plan = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: vec![root.join("child").to_string_lossy().into_owned()],
    });
    assert!(service.enqueue_automatic_request(plan));
    // Populate the explicit map with an active context for the same tab.
    let _ = service
        .sort_active_items(&ActiveItemsSortRequest {
            context: context(&root.to_string_lossy()),
            sort_direction: SortDirection::Asc,
            filter: String::new(),
            show_hidden: true,
        })
        .expect("sort items");
    assert_eq!(service.automatic_queue_len(), 1);

    // Cancelling an unrelated tab must retain the owning tab's queued work.
    service.cancel_tab("right-9");
    assert_eq!(service.automatic_queue_len(), 1);

    // Cancelling the owning tab drops its queued plan and active contexts.
    service.cancel_tab("left-1");
    assert_eq!(service.automatic_queue_len(), 0);
}

#[test]
fn invalidate_directory_generation_makes_a_counted_directory_recountable() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("child")).expect("child");
    fs::write(root.join("child").join("a"), b"a").expect("seed");
    let service = ItemCountService::default();
    let child = root.join("child").to_string_lossy().into_owned();

    // request_id 7 (from `context`) is the cache generation for the count.
    let plan = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: context(&root.to_string_lossy()),
        paths: vec![child.clone()],
    });
    service.process_automatic_request(plan, |_| {});
    assert_eq!(service.cache_len(), 1);

    service.invalidate_directory_generation(&child, 7);

    let replan = service.plan_automatic_request(&VisibleItemCountsRequest {
        context: ItemCountRequestContext {
            request_id: 7,
            ..context(&root.to_string_lossy())
        },
        paths: vec![child],
    });
    assert!(
        !replan.is_empty(),
        "an invalidated directory generation must require a fresh count"
    );
}

#[test]
fn generation_cache_coalesces_pending_work_and_invalidates_only_the_changed_directory() {
    let mut cache = ItemCountCache::new(4);
    assert!(cache.is_empty());
    assert!(cache.begin("/one", 3));
    assert!(!cache.begin("/one", 3));
    cache.resolve("/one", 3, ItemCountState::Exact { value: 2 });
    cache.resolve("/two", 3, ItemCountState::Exact { value: 7 });
    assert_eq!(cache.state("/one", 3).value(), Some(2));
    cache.invalidate_generation("/one", 3);
    assert_eq!(cache.state("/one", 3), ItemCountState::Unknown);
    assert_eq!(cache.state("/two", 3).value(), Some(7));
}

#[test]
fn item_count_cache_never_exceeds_approved_entry_limit() {
    let mut cache = ItemCountCache::default();
    for index in 0..(ITEM_COUNT_CACHE_LIMIT + 5) {
        cache.resolve(
            &format!("/count-{index}"),
            1,
            ItemCountState::Exact {
                value: index as u64,
            },
        );
    }
    assert_eq!(cache.len(), ITEM_COUNT_CACHE_LIMIT);
}

#[test]
fn items_sort_reuses_active_session_snapshot_and_viewport_count_cache() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let alpha = root.join("alpha");
    let beta = root.join("beta");
    fs::create_dir(&alpha).expect("alpha");
    fs::create_dir(&beta).expect("beta");
    fs::write(alpha.join("a"), b"a").expect("alpha child");
    fs::write(beta.join("a"), b"a").expect("beta child");
    fs::write(beta.join("b"), b"b").expect("beta child");

    let path = root.to_string_lossy().into_owned();
    let sessions = DirectorySessionService::default();
    sessions
        .begin_navigation(
            BeginNavigationRequest {
                pane_id: "left".to_string(),
                tab_id: "left-1".to_string(),
                path: path.clone(),
                view: ViewParams {
                    sort_key: SortKey::Name,
                    sort_direction: SortDirection::Asc,
                    filter: String::new(),
                    show_hidden: true,
                    include_item_counts: false,
                },
            },
            None,
            None,
        )
        .expect("start session");

    let service = ItemCountService::default();
    let generation = sessions
        .watch_revision_for_pane_path(&"left".to_string(), &path)
        .expect("active session generation");
    let plan = service.plan_automatic_request_with_generation(
        &VisibleItemCountsRequest {
            context: context(&path),
            paths: vec![
                alpha.to_string_lossy().into_owned(),
                beta.to_string_lossy().into_owned(),
            ],
        },
        generation,
    );
    service.process_automatic_request(plan, |_| {});

    let list_calls_before = explorer_fs::list_dir_calls_for_tests();
    let count_calls_before = explorer_fs::read_item_count_calls_for_tests();
    let response = service
        .sort_active_items_with_session(
            &ActiveItemsSortRequest {
                context: context(&path),
                sort_direction: SortDirection::Desc,
                filter: String::new(),
                show_hidden: true,
            },
            Some(&sessions),
        )
        .expect("sort from snapshot");

    let ActiveItemsSortResponse::Ready(ready) = response else {
        panic!("expected ready result");
    };
    assert_eq!(
        ready
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta", "alpha"]
    );
    assert_eq!(explorer_fs::list_dir_calls_for_tests(), list_calls_before);
    assert_eq!(
        explorer_fs::read_item_count_calls_for_tests(),
        count_calls_before
    );
}
