#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::size::everything::{EverythingAvailability, EverythingHandle};
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
fn capability_selection_prefers_network_na_and_manual_fallback() {
    let service = SizeService::new(Duration::from_secs(1));
    let network = VolumeInfo {
        mount_root: String::from("/network"),
        label: String::from("Network"),
        total_bytes: 1,
        free_bytes: 1,
        is_network: true,
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
fn size_service_reports_missing_manual_path_as_error() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing");
    let service = SizeService::new(Duration::from_millis(100));
    let updates = Arc::new(Mutex::new(Vec::<SizeUpdate>::new()));
    let updates_for_emitter = updates.clone();

    service.request_paths(vec![missing.to_string_lossy().into_owned()], move |update| {
        updates_for_emitter
            .lock()
            .expect("updates lock")
            .push(update);
    });

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
