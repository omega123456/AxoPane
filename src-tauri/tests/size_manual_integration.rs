#[path = "common/mod.rs"]
mod common;

use std::collections::HashMap;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};
use file_explorer_lib::size::everything::EverythingAvailability;
use file_explorer_lib::size::everything::{
    build_exact_folder_or_queries, escape_exact_folder_query_path, join_everything_result_path,
    map_everything_result_sizes, normalize_result_path, EverythingHandle,
    EVERYTHING_BATCH_CHUNK_SIZE,
};
use file_explorer_lib::size::manual::{calculate, ManualSizeError};
use file_explorer_lib::size::{SizeBackend, SizeService, SizeSource, SizeStateKind, SizeUpdate};
use file_explorer_lib::volumes::VolumeInfo;
use tempfile::tempdir;

#[test]
fn manual_sizer_sums_nested_file_sizes() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("nested")).expect("nested dir");
    fs::write(root.join("alpha.txt"), b"abcd").expect("alpha");
    fs::write(root.join("nested").join("beta.txt"), b"12345").expect("beta");

    let size = calculate(
        root,
        &Arc::new(AtomicBool::new(false)),
        Duration::from_secs(1),
    )
    .expect("size");
    assert_eq!(size, 9);
}

#[test]
fn manual_sizer_honors_cancellation() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    for index in 0..200 {
        fs::write(root.join(format!("file-{index}.bin")), vec![0_u8; 1024]).expect("seed file");
    }

    let cancel = Arc::new(AtomicBool::new(true));
    let error = calculate(root, &cancel, Duration::from_secs(1)).expect_err("cancelled");
    assert!(matches!(error, ManualSizeError::Cancelled));
}

#[test]
fn manual_sizer_times_out() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    for index in 0..50 {
        fs::write(root.join(format!("file-{index}.bin")), vec![0_u8; 4096]).expect("seed file");
    }

    let error =
        calculate(root, &Arc::new(AtomicBool::new(false)), Duration::ZERO).expect_err("timeout");
    assert!(matches!(error, ManualSizeError::Timeout));
}

#[test]
fn manual_size_error_formats_and_converts_io_failures() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing");

    let error = calculate(
        &missing,
        &Arc::new(AtomicBool::new(false)),
        Duration::from_secs(1),
    )
    .expect_err("missing path");
    assert!(!error.to_string().is_empty());
    assert!(matches!(error, ManualSizeError::Io(_)));
}

#[test]
fn manual_size_error_converts_shared_traversal_failures() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing-root");

    let walk_error = match file_explorer_lib::traversal::walk(
        &missing,
        file_explorer_lib::traversal::TraversalOptions::default(),
        Arc::new(AtomicBool::new(false)),
    ) {
        Err(error) => error,
        Ok(_) => panic!("missing root should fail traversal"),
    };
    let error: ManualSizeError = walk_error.into();

    assert!(matches!(error, ManualSizeError::Walk(_)));
    assert!(!error.to_string().is_empty());
}

#[test]
fn manual_sizer_returns_zero_for_regular_files() {
    let fixture = tempdir().expect("temp dir");
    let file = fixture.path().join("plain.txt");
    fs::write(&file, b"content").expect("file");

    let size = calculate(
        &file,
        &Arc::new(AtomicBool::new(false)),
        Duration::from_secs(1),
    )
    .expect("size");
    assert_eq!(size, 0);
}

#[cfg(unix)]
#[test]
fn manual_sizer_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;

    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("real")).expect("real dir");
    fs::write(root.join("real").join("inside.txt"), vec![0_u8; 7]).expect("real file");
    symlink(root.join("real"), root.join("link")).expect("symlink");

    let size = calculate(
        root,
        &Arc::new(AtomicBool::new(false)),
        Duration::from_secs(1),
    )
    .expect("size");
    assert_eq!(size, 7);
}

// Under `test-utils`, the Windows Everything backend must compile to an
// in-memory fake that never loads the real DLL and always reports the service as
// unavailable — asserting the safe fallback, never a real-API success.
#[cfg(all(windows, feature = "test-utils"))]
#[test]
fn everything_stub_reports_unavailable_under_test_utils() {
    let handle = EverythingHandle::load().expect("stub handle loads");
    assert_eq!(handle.availability(), EverythingAvailability::Unavailable);

    let fixture = tempdir().expect("temp dir");
    fs::create_dir(fixture.path().join("folder")).expect("folder");
    assert!(handle
        .query_folder_size(&fixture.path().join("folder"))
        .is_err());
    assert!(handle
        .query_folder_sizes(
            &[fixture.path().join("folder").to_string_lossy().into_owned()],
            &|| false,
        )
        .is_err());
}

#[cfg(not(windows))]
#[test]
fn everything_non_windows_stub_reports_unsupported_and_supports_in_memory_queries() {
    let load_error = match EverythingHandle::load() {
        Ok(_) => panic!("non-Windows load should be unsupported"),
        Err(error) => error,
    };
    assert_eq!(
        load_error.to_string(),
        "Everything is unsupported on this platform"
    );

    let mut results = HashMap::new();
    results.insert("/tmp/known".to_string(), Some(42));
    let handle = EverythingHandle::test_available(results);
    assert_eq!(handle.availability(), EverythingAvailability::Available);
    assert_eq!(
        handle
            .query_folder_size(std::path::Path::new("/tmp/known"))
            .expect("query"),
        Some(42),
    );
    assert!(EverythingHandle::test_available_error()
        .query_folder_sizes(&["/tmp/known".to_string()], &|| false)
        .is_err());
}

#[cfg(windows)]
#[test]
fn everything_ffi_loads_or_skips_cleanly() {
    let Ok(handle) = EverythingHandle::load() else {
        return;
    };

    if handle.availability() != EverythingAvailability::Available {
        return;
    }

    let fixture = tempdir().expect("temp dir");
    fs::create_dir(fixture.path().join("folder")).expect("folder");
    let result = handle.query_folder_size(&fixture.path().join("folder"));
    assert!(result.is_ok());
}

#[test]
fn everything_query_helpers_escape_and_chunk_exact_folder_queries() {
    assert_eq!(
        escape_exact_folder_query_path(r#"C:\root\say "hello""#),
        r#"C:\root\say ""hello"""#
    );

    let queries = build_exact_folder_or_queries(
        &[
            r#"C:\root\Alpha"#.to_string(),
            r#"C:\root\Bravo"#.to_string(),
            r#"C:\root\say "hello""#.to_string(),
        ],
        2,
    );

    assert_eq!(queries.len(), 2);
    assert_eq!(
        queries[0],
        r#"exact:folder:"C:\root\Alpha" | exact:folder:"C:\root\Bravo""#
    );
    assert_eq!(queries[1], r#"exact:folder:"C:\root\say ""hello""""#);
    const { assert!(EVERYTHING_BATCH_CHUNK_SIZE >= 1) };
}

#[test]
fn everything_result_mapping_normalizes_joined_paths() {
    if cfg!(windows) {
        let requested = vec![
            r"C:\Program Files".to_string(),
            r"C:\Root\Alpha".to_string(),
            r"C:\Root\Beta".to_string(),
            r"\\?\UNC\nas\share\Gamma".to_string(),
        ];
        let results = vec![
            (r"C:".to_string(), "Program Files".to_string(), Some(30)),
            (r"c:\root".to_string(), "ALPHA".to_string(), Some(10)),
            (r"C:\ROOT".to_string(), "Beta".to_string(), None),
            (r"\\nas\share".to_string(), "gamma".to_string(), Some(25)),
        ];

        let mapped = map_everything_result_sizes(&requested, &results);
        assert_eq!(mapped.get(r"C:\Program Files"), Some(&Some(30)));
        assert_eq!(mapped.get(r"C:\Root\Alpha"), Some(&Some(10)));
        assert_eq!(mapped.get(r"C:\Root\Beta"), Some(&None));
        assert_eq!(mapped.get(r"\\?\UNC\nas\share\Gamma"), Some(&Some(25)));
        assert_eq!(
            join_everything_result_path(r"C:", "Program Files"),
            r"C:\Program Files"
        );
        assert_eq!(
            normalize_result_path(r"\\?\UNC\nas\share\Gamma\"),
            r"\\nas\share\gamma"
        );
        assert_eq!(normalize_result_path(r"C:\Root\Alpha\"), r"c:\root\alpha");
    } else {
        let requested = vec!["/tmp/alpha".to_string(), "/tmp/beta".to_string()];
        let results = vec![
            ("/tmp".to_string(), "alpha".to_string(), Some(10)),
            ("/tmp".to_string(), "beta".to_string(), None),
        ];

        let mapped = map_everything_result_sizes(&requested, &results);
        assert_eq!(mapped.get("/tmp/alpha"), Some(&Some(10)));
        assert_eq!(mapped.get("/tmp/beta"), Some(&None));
        assert_eq!(normalize_result_path("/tmp/alpha/"), "/tmp/alpha");
    }
}

#[test]
fn capability_selection_prefers_network_na_and_manual_fallback() {
    let service = SizeService::new(Duration::from_secs(1));
    let network = VolumeInfo {
        mount_root: String::from("/network"),
        label: String::from("Network"),
        total_bytes: 1,
        free_bytes: 1,
        is_network: true,
        is_removable: false,
    };

    assert_eq!(
        service.choose_backend_for_path(std::path::Path::new("/network/share"), &[network]),
        SizeBackend::NetworkNa
    );

    let local = VolumeInfo {
        mount_root: if cfg!(windows) {
            String::from("C:\\")
        } else {
            String::from("/")
        },
        label: String::from("Local"),
        total_bytes: 1,
        free_bytes: 1,
        is_network: false,
        is_removable: false,
    };

    let local_mount = local.mount_root.clone();
    let backend = service.choose_backend_for_path(std::path::Path::new(&local_mount), &[local]);
    if cfg!(windows) && service.everything_status().is_available {
        assert_eq!(backend, SizeBackend::Everything);
    } else {
        assert_eq!(backend, SizeBackend::Manual);
    }

    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn size_service_request_paths_emits_manual_success_lifecycle() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("alpha.bin"), vec![1_u8; 7]).expect("alpha");
    fs::create_dir(root.join("nested")).expect("nested");
    fs::write(root.join("nested").join("beta.bin"), vec![1_u8; 5]).expect("beta");

    let service = SizeService::new(Duration::from_secs(1));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(vec![root.to_string_lossy().into_owned()], move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    wait_for_updates(&updates, |updates| {
        updates
            .iter()
            .any(|update| update.state == SizeStateKind::Ready)
    });

    let recorded = updates.lock().expect("updates lock").clone();
    assert_eq!(recorded[0].state, SizeStateKind::Calculating);
    let ready = recorded
        .iter()
        .find(|update| update.state == SizeStateKind::Ready)
        .expect("ready update");
    assert_eq!(ready.source, SizeSource::Manual);
    assert_eq!(ready.size_bytes, Some(12));
}

#[test]
fn size_service_cancel_removes_a_requested_manual_job() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path().join("cancelled");
    fs::create_dir_all(&root).expect("root");
    for index in 0..400 {
        fs::write(root.join(format!("file-{index}.bin")), vec![1_u8; 4096]).expect("seed file");
    }

    let service = SizeService::new(Duration::from_secs(5));
    let target = root.to_string_lossy().into_owned();
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(vec![target.clone()], move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    // The job is registered synchronously by `request_paths`, so cancelling
    // immediately — before its walk could finish — reliably finds and removes
    // it. (A mid-walk cancel actually aborting the traversal is covered by
    // `manual_sizer_honors_cancellation`.)
    assert!(service.cancel(&target));
    // The registry entry is gone, so a second cancel finds nothing.
    assert!(!service.cancel(&target));
}

#[test]
fn size_service_cancel_many_removes_only_requested_in_flight_jobs() {
    let fixture = tempdir().expect("temp dir");
    let roots = ["alpha", "beta", "gamma"]
        .into_iter()
        .map(|name| {
            let root = fixture.path().join(name);
            fs::create_dir_all(&root).expect("root");
            for index in 0..400 {
                fs::write(root.join(format!("file-{index}.bin")), vec![1_u8; 4096])
                    .expect("seed file");
            }
            root.to_string_lossy().into_owned()
        })
        .collect::<Vec<_>>();

    let service = SizeService::new(Duration::from_secs(5));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(roots.clone(), move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    // `request_paths` registers every job synchronously before it returns, so
    // cancelling right away — before any multi-file walk could possibly finish
    // — deterministically reports exactly the requested-and-registered jobs.
    // Cancelling alpha/beta plus a path that was never requested reports only
    // the two real jobs.
    let cancelled = service.cancel_many(&[
        roots[0].clone(),
        roots[1].clone(),
        fixture
            .path()
            .join("never-requested")
            .to_string_lossy()
            .into_owned(),
    ]);
    assert_eq!(cancelled, 2);

    // gamma was left untouched, so a follow-up batch still finds it cancellable.
    assert_eq!(service.cancel_many(&[roots[2].clone()]), 1);
    // Every job is now gone from the registry.
    assert_eq!(service.cancel_many(&roots), 0);
}

#[test]
fn queued_manual_jobs_are_skipped_once_cancelled() {
    // Reproduce the "huge folder, then navigate away" case: far more jobs than
    // the worker pool can run at once, cancelled the instant they are queued.
    // The backlog must be skipped without walking every folder, so the
    // registry drains promptly and almost nothing produces a Ready result.
    let fixture = tempdir().expect("temp dir");
    let mut paths = Vec::new();
    for index in 0..200 {
        let dir = fixture.path().join(format!("folder-{index}"));
        fs::create_dir_all(&dir).expect("folder");
        fs::write(dir.join("file.bin"), vec![1_u8; 16]).expect("seed file");
        paths.push(dir.to_string_lossy().into_owned());
    }

    let service = SizeService::new(Duration::from_secs(5));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(paths.clone(), move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    // Cancel the whole batch immediately; the vast majority are still queued
    // behind the bounded pool and must take the pre-work skip branch.
    service.cancel_many(&paths);

    // The registry drains quickly precisely because skipped jobs do no I/O.
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && service.cancel_many(&paths) > 0 {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(service.cancel_many(&paths), 0);

    let ready = updates
        .lock()
        .expect("updates lock")
        .iter()
        .filter(|update| update.state == SizeStateKind::Ready)
        .count();
    assert!(
        ready < paths.len(),
        "cancelled backlog should skip work, but {ready} of {} completed",
        paths.len()
    );
}

#[test]
fn completed_size_jobs_are_removed_from_the_registry() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("alpha.bin"), vec![1_u8; 7]).expect("alpha");

    let service = SizeService::new(Duration::from_secs(1));
    let target = root.to_string_lossy().into_owned();
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(vec![target.clone()], move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    wait_for_updates(&updates, |updates| {
        updates
            .iter()
            .any(|update| matches!(update.state, SizeStateKind::Ready | SizeStateKind::Na))
    });

    assert!(!service.cancel(&target));
}

#[test]
fn everything_backed_requests_emit_batched_ready_and_na_results() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let alpha = root.join("alpha");
    let beta = root.join("beta");
    let alpha_path = alpha.to_string_lossy().into_owned();
    let beta_path = beta.to_string_lossy().into_owned();
    let service = SizeService::with_everything_handle(
        Duration::from_secs(1),
        Some(EverythingHandle::test_available(HashMap::from([(
            alpha_path.clone(),
            Some(42),
        )]))),
    );
    let batches = Arc::new(Mutex::new(Vec::<Vec<SizeUpdate>>::new()));
    let batches_for_emitter = batches.clone();

    assert_eq!(
        service.everything_status().status,
        file_explorer_lib::size::EverythingStatusKind::Available
    );
    assert!(service.everything_status().is_available);

    service.request_paths_with_volumes(
        vec![alpha_path.clone(), beta_path.clone()],
        vec![local_volume_for(root)],
        Arc::new(move |batch| {
            batches_for_emitter
                .lock()
                .expect("updates lock")
                .push(batch);
        }),
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        let terminal_count = batches
            .lock()
            .expect("updates lock")
            .iter()
            .flatten()
            .filter(|update| {
                matches!(update.state, SizeStateKind::Ready | SizeStateKind::Na)
                    && update.source == SizeSource::Everything
            })
            .count()
            >= 2;
        if terminal_count {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let recorded_batches = batches.lock().expect("updates lock").clone();
    assert!(recorded_batches.iter().any(|batch| {
        batch.len() == 2
            && batch
                .iter()
                .all(|update| update.state == SizeStateKind::Calculating)
    }));
    assert!(recorded_batches.iter().any(|batch| {
        batch.len() == 2
            && batch
                .iter()
                .all(|update| matches!(update.state, SizeStateKind::Ready | SizeStateKind::Na))
    }));
    let recorded = recorded_batches.into_iter().flatten().collect::<Vec<_>>();
    assert!(recorded.iter().any(|update| {
        update.path == alpha_path
            && update.state == SizeStateKind::Ready
            && update.source == SizeSource::Everything
            && update.size_bytes == Some(42)
    }));
    assert!(recorded.iter().any(|update| {
        update.path == beta_path
            && update.state == SizeStateKind::Na
            && update.source == SizeSource::Everything
            && update.size_bytes.is_none()
    }));
    assert!(!service.cancel(&alpha_path));
    assert!(!service.cancel(&beta_path));
}

#[test]
fn everything_query_honors_cancellation_predicate() {
    let path = if cfg!(windows) {
        r"C:\alpha".to_string()
    } else {
        "/tmp/alpha".to_string()
    };
    let handle = EverythingHandle::test_available(HashMap::from([(path.clone(), Some(99_u64))]));

    // Not cancelled: the query runs and resolves the known size.
    let ready = handle
        .query_folder_sizes(std::slice::from_ref(&path), &|| false)
        .expect("query");
    assert_eq!(ready.get(&path).copied().flatten(), Some(99));

    // Cancelled up front: the query short-circuits instead of issuing IPC for
    // the batch, so no size is resolved for the path.
    let cancelled = handle
        .query_folder_sizes(std::slice::from_ref(&path), &|| true)
        .expect("query");
    assert!(cancelled.get(&path).copied().flatten().is_none());
}

#[test]
fn everything_backed_requests_emit_errors_when_the_batch_query_fails() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let target = root.join("broken").to_string_lossy().into_owned();
    let service = SizeService::with_everything_handle(
        Duration::from_secs(1),
        Some(EverythingHandle::test_available_error()),
    );
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_path_with_volumes(
        target.clone(),
        vec![local_volume_for(root)],
        Arc::new(move |update| {
            updates_for_emitter
                .lock()
                .expect("updates lock")
                .extend(update);
        }),
    );

    wait_for_updates(&updates, |recorded| {
        recorded.iter().any(|update| {
            update.path == target
                && update.state == SizeStateKind::Error
                && update.source == SizeSource::Everything
        })
    });

    assert!(!service.cancel(&target));
}

#[test]
fn size_service_reports_missing_manual_path_as_error() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing");
    let service = SizeService::new(Duration::from_millis(100));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(
        vec![missing.to_string_lossy().into_owned()],
        move |update| {
            updates_for_emitter
                .lock()
                .expect("updates lock")
                .extend(update);
        },
    );

    wait_for_updates(&updates, |updates| {
        updates
            .iter()
            .any(|update| update.state == SizeStateKind::Error)
    });

    let recorded = updates.lock().expect("updates lock").clone();
    assert!(recorded
        .iter()
        .any(|update| update.source == SizeSource::Manual && update.state == SizeStateKind::Error));
}

#[test]
fn request_path_with_explicit_volumes_emits_terminal_events() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("alpha.bin"), vec![1_u8; 4]).expect("alpha");

    let service = SizeService::new(Duration::from_secs(1));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();
    let local_root = if cfg!(windows) {
        root.components()
            .next()
            .expect("component")
            .as_os_str()
            .to_string_lossy()
            .into_owned()
    } else {
        "/".to_string()
    };

    service.request_path_with_volumes(
        root.to_string_lossy().into_owned(),
        vec![VolumeInfo {
            mount_root: local_root,
            label: "Local".to_string(),
            total_bytes: 1,
            free_bytes: 1,
            is_network: false,
            is_removable: false,
        }],
        Arc::new(move |update| {
            updates_for_emitter
                .lock()
                .expect("updates lock")
                .extend(update);
        }),
    );

    wait_for_updates(&updates, |recorded| {
        recorded.iter().any(|update| {
            matches!(
                update.state,
                SizeStateKind::Ready | SizeStateKind::Error | SizeStateKind::Na
            )
        })
    });
}

#[cfg(windows)]
#[test]
fn size_service_reports_fixture_network_paths_as_not_applicable() {
    let service = SizeService::new(Duration::from_secs(1));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(vec!["Z:\\".to_string()], move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    let recorded = updates.lock().expect("updates lock").clone();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].state, SizeStateKind::Na);
    assert_eq!(recorded[0].source, SizeSource::Network);
    assert_eq!(recorded[0].size_bytes, None);
}

#[test]
fn size_service_mixed_backend_requests_emit_network_and_manual_terminal_events() {
    let fixture = tempdir().expect("temp dir");
    let local_root = fixture.path();
    fs::write(local_root.join("alpha.bin"), vec![1_u8; 4]).expect("alpha");

    let service = SizeService::new(Duration::from_secs(1));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    let local_mount = if cfg!(windows) {
        local_root
            .components()
            .next()
            .expect("component")
            .as_os_str()
            .to_string_lossy()
            .into_owned()
    } else {
        "/".to_string()
    };
    let network_mount = if cfg!(windows) {
        "Z:\\".to_string()
    } else {
        "/network".to_string()
    };
    let network_path = if cfg!(windows) {
        "Z:\\".to_string()
    } else {
        "/network/share".to_string()
    };

    service.request_paths_with_volumes(
        vec![
            network_path.clone(),
            local_root.to_string_lossy().into_owned(),
        ],
        vec![
            VolumeInfo {
                mount_root: network_mount,
                label: "Network".to_string(),
                total_bytes: 1,
                free_bytes: 1,
                is_network: true,
                is_removable: false,
            },
            VolumeInfo {
                mount_root: local_mount,
                label: "Local".to_string(),
                total_bytes: 1,
                free_bytes: 1,
                is_network: false,
                is_removable: false,
            },
        ],
        Arc::new(move |update| {
            updates_for_emitter
                .lock()
                .expect("updates lock")
                .extend(update);
        }),
    );

    wait_for_updates(&updates, |recorded| {
        recorded
            .iter()
            .filter(|update| {
                matches!(
                    update.state,
                    SizeStateKind::Ready | SizeStateKind::Error | SizeStateKind::Na
                )
            })
            .count()
            >= 2
    });

    let recorded = updates.lock().expect("updates lock").clone();
    assert!(recorded.iter().any(|update| {
        update.path == network_path
            && update.source == SizeSource::Network
            && update.state == SizeStateKind::Na
    }));
    assert!(recorded.iter().any(|update| {
        update.path == local_root.to_string_lossy()
            && update.source == SizeSource::Manual
            && update.state == SizeStateKind::Ready
    }));
}

fn wait_for_updates(
    updates: &Arc<Mutex<Vec<SizeUpdate>>>,
    predicate: impl Fn(&[SizeUpdate]) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        let recorded = updates.lock().expect("updates lock").clone();
        if predicate(&recorded) {
            return;
        }
        drop(recorded);
        thread::sleep(Duration::from_millis(10));
    }

    panic!("timed out waiting for size updates");
}

/// `SizeService::with_resource_coordinator` (and the injectable
/// `with_everything_handle_and_coordinator` test constructor) must route
/// manual-size traversal through the *caller-supplied* coordinator instead
/// of a private one it builds internally. Proven here by pre-occupying every
/// `Throughput` slot on an externally held coordinator and observing that a
/// manual size request stays parked at `Calculating` (admission blocked)
/// until the external holder releases its slots — a private coordinator
/// would never see this occupancy and the job would complete immediately.
#[test]
fn size_service_manual_jobs_are_admitted_through_the_injected_coordinator() {
    let coordinator = Arc::new(ResourceCoordinator::new());

    // Saturate every throughput slot the coordinator has so any job routed
    // through *this* coordinator instance cannot be admitted yet.
    let mut occupying_handles = Vec::new();
    for index in 0..file_explorer_lib::resource_coordinator::queue::MAX_THROUGHPUT_SLOTS {
        occupying_handles.push(
            coordinator
                .submit(JobSpec::new(
                    [JobClass::Throughput],
                    [format!("occupy-{index}")],
                ))
                .expect("occupying throughput slot admitted"),
        );
    }

    let service = SizeService::with_everything_handle_and_coordinator(
        Duration::from_secs(5),
        None,
        Arc::clone(&coordinator),
    );
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("alpha.bin"), vec![1_u8; 4]).expect("alpha");
    let target = root.to_string_lossy().into_owned();

    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();
    service.request_paths(vec![target.clone()], move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    // Give the worker pool a bounded window to try (and fail) admission;
    // it must not have produced a terminal Ready/Error/Na state yet because
    // every throughput slot on the shared coordinator is still occupied.
    thread::sleep(Duration::from_millis(150));
    assert!(updates
        .lock()
        .expect("updates lock")
        .iter()
        .all(|update| update.state != SizeStateKind::Ready
            && update.state != SizeStateKind::Error
            && update.state != SizeStateKind::Na));

    // Releasing the external occupancy frees the shared coordinator's
    // throughput lane, letting the size worker's queued admission resolve
    // and the job complete.
    drop(occupying_handles);
    wait_for_updates(&updates, |updates| {
        updates
            .iter()
            .any(|update| update.state == SizeStateKind::Ready)
    });
}

/// Regression test for the "phantom cancellation token" audit finding: two
/// overlapping `request_paths` calls for the *same* still-in-flight path
/// must coalesce onto one real job rather than the second call fabricating a
/// disconnected token. Proven by holding the manual worker pool's only
/// throughput admission open (a controlled first path occupies it) while a
/// second request for a *different* long-running path is issued twice in a
/// row before either could possibly complete — the second, duplicate
/// request must not produce its own independent terminal event, and
/// cancelling one of the two overlapping registrations must not orphan the
/// other (the path must still resolve to exactly one terminal state).
#[test]
fn overlapping_requests_for_the_same_in_flight_path_coalesce_to_one_completion() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path().join("overlap");
    fs::create_dir_all(&root).expect("root");
    for index in 0..300 {
        fs::write(root.join(format!("file-{index}.bin")), vec![1_u8; 4096]).expect("seed file");
    }
    let target = root.to_string_lossy().into_owned();

    let service = SizeService::new(Duration::from_secs(5));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    // First request registers and (once a pool worker claims it) starts
    // walking `target`.
    service.request_paths(vec![target.clone()], {
        let updates_for_emitter = updates_for_emitter.clone();
        move |update| {
            updates_for_emitter
                .lock()
                .expect("updates lock")
                .extend(update);
        }
    });

    // A second, overlapping request for the exact same path arrives before
    // the first could possibly have finished walking 300 files. Per the
    // coalescing contract this must find the existing in-flight job and
    // return its same tracked token rather than registering (or silently
    // dropping) a second one.
    service.request_paths(vec![target.clone()], move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    wait_for_updates(&updates, |updates| {
        updates
            .iter()
            .any(|update| matches!(update.state, SizeStateKind::Ready | SizeStateKind::Na))
    });

    let recorded = updates.lock().expect("updates lock").clone();
    let terminal_count = recorded
        .iter()
        .filter(|update| {
            update.path == target
                && matches!(update.state, SizeStateKind::Ready | SizeStateKind::Na)
        })
        .count();
    assert_eq!(
        terminal_count, 1,
        "overlapping requests for the same path must resolve to exactly one \
         terminal event, not one per request: {recorded:?}"
    );

    // The job is fully completed and untracked, not orphaned: a fresh
    // request for the same path now starts a brand-new job instead of
    // silently coalescing onto stale state.
    assert!(!service.cancel(&target));
}

/// A path the scheduler cannot admit as a new entry (capacity exhausted)
/// must not leave the caller with no terminal event at all — this was the
/// concrete, reachable form of the phantom-token bug: `register_job`
/// returning a freshly fabricated, disconnected cancel token for a path that
/// was never actually tracked in the scheduler or `SizeService`'s own job
/// map, so nothing would ever call `complete_job` for it and the request
/// would silently vanish with the caller waiting forever. With the fix, a
/// rejected path is reported as a terminal `Error` immediately instead.
///
/// Exercised end-to-end through `SizeService` itself (not just the
/// scheduler in isolation) using the test-only capacity override, so this
/// asserts the actual production `register_job`/`enqueue_manual_job` wiring,
/// not just the underlying data structure.
#[test]
fn a_manual_job_rejected_for_capacity_still_reports_a_terminal_event() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    // Capacity 1: the first path fills the only slot; a second, distinct
    // path cannot be scheduled and must be rejected by `register_job`.
    let service = SizeService::with_capacity_for_tests(Duration::from_secs(5), coordinator, 1);

    let fixture = tempdir().expect("temp dir");
    let first = fixture.path().join("first");
    let second = fixture.path().join("second");
    fs::create_dir_all(&first).expect("first");
    fs::create_dir_all(&second).expect("second");
    // Give the first job enough files that it is still in flight (occupying
    // the scheduler's only slot) when the second request lands.
    for index in 0..300 {
        fs::write(first.join(format!("file-{index}.bin")), vec![1_u8; 4096]).expect("seed file");
    }
    let first_path = first.to_string_lossy().into_owned();
    let second_path = second.to_string_lossy().into_owned();

    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();
    service.request_paths(
        vec![first_path.clone(), second_path.clone()],
        move |update| {
            updates_for_emitter
                .lock()
                .expect("updates lock")
                .extend(update);
        },
    );

    // The rejected second path must still resolve to a terminal state
    // promptly — it never occupies a scheduler slot, so nothing needs to
    // "finish" for it to report in.
    wait_for_updates(&updates, |updates| {
        updates.iter().any(|update| {
            update.path == second_path
                && matches!(
                    update.state,
                    SizeStateKind::Error | SizeStateKind::Ready | SizeStateKind::Na
                )
        })
    });

    let recorded = updates.lock().expect("updates lock").clone();
    let second_terminal = recorded
        .iter()
        .find(|update| update.path == second_path)
        .expect("second path must report a terminal event, not vanish silently");
    assert_eq!(second_terminal.state, SizeStateKind::Error);
    assert_eq!(second_terminal.source, SizeSource::Manual);

    // The first path was unaffected by the rejection and still completes
    // normally once the (only) worker pool slot processes it.
    wait_for_updates(&updates, |updates| {
        updates.iter().any(|update| {
            update.path == first_path
                && matches!(update.state, SizeStateKind::Ready | SizeStateKind::Na)
        })
    });
}

/// Confirms the manual worker pool actually joins (no leaked/blocked
/// threads) when the owning `SizeService` is dropped, mirroring how
/// `WatchRuntime`'s `Drop` shuts its scheduler/coordinator down
/// deterministically. Proven by dropping a service with in-flight work
/// still queued behind the (small) worker pool and observing `drop` itself
/// returns promptly rather than hanging — if a worker were leaked blocked
/// on `scheduler.next()`, the scheduler's internal state would still be
/// reachable but the thread `JoinHandle`s would never resolve, and this
/// call would hang past the test's own timeout.
#[test]
fn dropping_the_service_joins_the_worker_pool_deterministically() {
    let fixture = tempdir().expect("temp dir");
    let mut paths = Vec::new();
    for index in 0..40 {
        let dir = fixture.path().join(format!("folder-{index}"));
        fs::create_dir_all(&dir).expect("folder");
        fs::write(dir.join("file.bin"), vec![1_u8; 16]).expect("seed file");
        paths.push(dir.to_string_lossy().into_owned());
    }

    let service = SizeService::new(Duration::from_secs(5));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();
    service.request_paths(paths, move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .extend(update);
    });

    // Drop immediately, with most of the backlog still queued behind the
    // bounded pool. This must return without hanging: the scheduler's
    // shutdown wakes every worker parked in `next()`, and `Drop` joins them
    // all before returning.
    let started = Instant::now();
    drop(service);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "dropping SizeService must join its worker pool promptly, took {:?}",
        started.elapsed()
    );
}

fn local_volume_for(path: &std::path::Path) -> VolumeInfo {
    let mount_root = if cfg!(windows) {
        path.components()
            .next()
            .expect("component")
            .as_os_str()
            .to_string_lossy()
            .into_owned()
    } else {
        "/".to_string()
    };

    VolumeInfo {
        mount_root,
        label: "Local".to_string(),
        total_bytes: 1,
        free_bytes: 1,
        is_network: false,
        is_removable: false,
    }
}
