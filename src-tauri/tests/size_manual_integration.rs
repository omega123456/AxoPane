#[path = "common/mod.rs"]
mod common;

use std::collections::HashMap;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
fn manual_size_error_converts_jwalk_failures() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing-root");

    let mut iterator = jwalk::WalkDir::new(&missing)
        .try_into_iter()
        .expect("iterator construction");
    let walk_error = match iterator.next() {
        Some(Err(error)) => error,
        other => panic!("missing root should yield a jwalk error, got {other:?}"),
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
        .query_folder_sizes(&[fixture.path().join("folder").to_string_lossy().into_owned()])
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
        .query_folder_sizes(&["/tmp/known".to_string()])
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
            .push(update);
    });

    wait_for_updates(&updates, |updates| {
        updates
            .iter()
            .any(|update| update.state == SizeStateKind::Ready)
    });

    let recorded = updates.lock().expect("updates lock").clone();
    assert_eq!(recorded[0].state, SizeStateKind::Unknown);
    assert_eq!(recorded[1].state, SizeStateKind::Calculating);
    let ready = recorded
        .iter()
        .find(|update| update.state == SizeStateKind::Ready)
        .expect("ready update");
    assert_eq!(ready.source, SizeSource::Manual);
    assert_eq!(ready.size_bytes, Some(12));
}

#[test]
fn size_service_cancel_removes_in_flight_manual_jobs() {
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
            .push(update);
    });

    wait_for_updates(&updates, |recorded| {
        recorded
            .iter()
            .any(|update| update.state == SizeStateKind::Calculating)
    });

    assert!(service.cancel(&target));

    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }

    assert!(!service.cancel(&target));
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
            .push(update);
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
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    assert_eq!(
        service.everything_status().status,
        file_explorer_lib::size::EverythingStatusKind::Available
    );
    assert!(service.everything_status().is_available);

    service.request_paths_with_volumes(
        vec![alpha_path.clone(), beta_path.clone()],
        vec![local_volume_for(root)],
        Arc::new(move |update| {
            updates_for_emitter
                .lock()
                .expect("updates lock")
                .push(update);
        }),
    );

    wait_for_updates(&updates, |recorded| {
        recorded
            .iter()
            .filter(|update| {
                matches!(update.state, SizeStateKind::Ready | SizeStateKind::Na)
                    && update.source == SizeSource::Everything
            })
            .count()
            >= 2
    });

    let recorded = updates.lock().expect("updates lock").clone();
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
                .push(update);
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
                .push(update);
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
                .push(update);
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
            .push(update);
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
                .push(update);
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
