#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use file_explorer_lib::size::everything::{EverythingAvailability, EverythingHandle};
use file_explorer_lib::size::manual::{calculate, ManualSizeError};
use file_explorer_lib::size::{SizeBackend, SizeService};
use file_explorer_lib::volumes::VolumeInfo;
use tempfile::tempdir;

#[test]
fn manual_sizer_sums_nested_file_sizes() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("nested")).expect("nested dir");
    fs::write(root.join("alpha.txt"), b"abcd").expect("alpha");
    fs::write(root.join("nested").join("beta.txt"), b"12345").expect("beta");

    let size = calculate(root, &Arc::new(AtomicBool::new(false)), Duration::from_secs(1)).expect("size");
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

    let error = calculate(root, &Arc::new(AtomicBool::new(false)), Duration::ZERO).expect_err("timeout");
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

    let size = calculate(root, &Arc::new(AtomicBool::new(false)), Duration::from_secs(1)).expect("size");
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
    assert!(handle.query_folder_size(&fixture.path().join("folder")).is_err());
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
        mount_root: if cfg!(windows) { String::from("C:\\") } else { String::from("/") },
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
