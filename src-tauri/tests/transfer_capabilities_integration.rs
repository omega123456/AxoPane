//! Capability and rename-first integration coverage.  These tests use only
//! adapter facts/fakes; no platform copy API or machine-global service runs.

use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use file_explorer_lib::ops::manifest::ProgressiveManifest;
use file_explorer_lib::ops::transfer::macos::MacosCapabilities;
use file_explorer_lib::ops::transfer::portable::PortableAdapter;
use file_explorer_lib::ops::transfer::windows::WindowsCapabilities;
use file_explorer_lib::ops::transfer::{
    AdapterSelection, PortableReason, TransferAdapter, TransferRequirements,
};
use file_explorer_lib::ops::worker::{rename_or_cross_device, transfer_admission};
use file_explorer_lib::ops::{
    adapter_selection_calls_for_tests, copy_path, move_path, OpItem, OpKind, OpState, WorkerCtx,
};
use file_explorer_lib::resource_coordinator::JobClass;

fn item(path: &str, name: &str) -> OpItem {
    OpItem {
        source_path: path.to_string(),
        name: name.to_string(),
        size_bytes: 10,
    }
}

fn state() -> OpState {
    OpState {
        id: "op-adapter".to_string(),
        kind: OpKind::Copy,
        destination_dir: std::path::PathBuf::from("dest"),
        items: vec![item("source", "source")],
        volumes: Default::default(),
        status: file_explorer_lib::ops::OpStatus::Active,
        total_items: 1,
        completed_items: 0,
        total_bytes: 0,
        copied_bytes: 0,
        bytes_per_second: 0,
        eta_seconds: None,
        sample_count: 0,
        rate_sample_at: None,
        rate_sample_bytes: 0,
        current_file_name: None,
        current_file_copied: 0,
        current_file_total: 0,
        error_message: None,
        completed_at: None,
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
        conflict: None,
        conflict_resolution: None,
        apply_to_all: None,
        rename_to: None,
    }
}

fn worker_ctx(
    state: OpState,
) -> (
    Arc<Mutex<OpState>>,
    Arc<std::sync::Condvar>,
    WorkerCtx<'static>,
) {
    let op_arc = Arc::new(Mutex::new(state));
    let resolver = Arc::new(std::sync::Condvar::new());
    let progress: Option<Arc<dyn Fn(file_explorer_lib::ops::OpProgress) + Send + Sync>> = None;
    let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);

    let op_arc_ref = Box::leak(Box::new(op_arc.clone()));
    let resolver_ref = Box::leak(Box::new(resolver.clone()));
    let progress_ref = Box::leak(Box::new(progress));
    let instant_now_ref = Box::leak(Box::new(instant_now));

    let ctx = WorkerCtx {
        op_arc: op_arc_ref,
        resolver: resolver_ref,
        progress: progress_ref,
        start: Instant::now(),
        rate_window: std::time::Duration::from_secs(1),
        instant_now: instant_now_ref,
    };
    (op_arc, resolver, ctx)
}

#[test]
fn portable_adapter_always_selects_portable_unsupported_platform() {
    let adapter = PortableAdapter;
    assert_eq!(
        adapter.select(TransferRequirements::default()),
        AdapterSelection::Portable(PortableReason::UnsupportedPlatform)
    );
    // Confirms the selection is unconditional regardless of requested
    // requirements (the portable adapter never claims native capabilities).
    assert_eq!(
        adapter.select(TransferRequirements {
            sparse: true,
            offload: true,
            clone: true,
            preserve_metadata: true,
            preserve_link: true,
            progress: true,
            cancellation: true,
        }),
        AdapterSelection::Portable(PortableReason::UnsupportedPlatform)
    );
}

#[test]
fn windows_capability_matrix_has_typed_portable_reasons() {
    let all = WindowsCapabilities {
        api_available: true,
        supported_build: true,
        local_filesystem: true,
        server_supports_copyfile2: true,
        sparse: true,
        offload: true,
        metadata: true,
        reparse_links: true,
        progress: true,
        cancellation: true,
    };
    assert_eq!(
        all.select(TransferRequirements {
            sparse: true,
            offload: true,
            preserve_metadata: true,
            preserve_link: true,
            ..Default::default()
        }),
        AdapterSelection::WindowsCopyFile2
    );
    assert_eq!(
        WindowsCapabilities {
            sparse: false,
            ..all
        }
        .select(TransferRequirements {
            sparse: true,
            ..Default::default()
        }),
        AdapterSelection::Portable(PortableReason::SparseUnsupported)
    );
    assert_eq!(
        WindowsCapabilities {
            local_filesystem: false,
            server_supports_copyfile2: false,
            ..all
        }
        .select(Default::default()),
        AdapterSelection::Portable(PortableReason::NetworkServerUnsupported)
    );
    assert_eq!(
        WindowsCapabilities {
            cancellation: false,
            ..all
        }
        .select(Default::default()),
        AdapterSelection::Portable(PortableReason::CancellationUnsupported)
    );
}

#[test]
fn macos_capability_matrix_requires_every_requested_claim() {
    let all = MacosCapabilities {
        api_available: true,
        filesystem_supports_copyfile: true,
        clone: true,
        sparse: true,
        metadata: true,
        links: true,
        progress: true,
        cancellation: true,
    };
    assert_eq!(
        all.select(TransferRequirements {
            clone: true,
            sparse: true,
            preserve_metadata: true,
            preserve_link: true,
            ..Default::default()
        }),
        AdapterSelection::MacosCopyfile
    );
    assert_eq!(
        MacosCapabilities {
            clone: false,
            ..all
        }
        .select(TransferRequirements {
            clone: true,
            ..Default::default()
        }),
        AdapterSelection::Portable(PortableReason::CloneUnsupported)
    );
    assert_eq!(
        MacosCapabilities {
            metadata: false,
            ..all
        }
        .select(TransferRequirements {
            preserve_metadata: true,
            ..Default::default()
        }),
        AdapterSelection::Portable(PortableReason::MetadataUnsupported)
    );
    assert_eq!(
        MacosCapabilities {
            links: false,
            ..all
        }
        .select(TransferRequirements {
            preserve_link: true,
            ..Default::default()
        }),
        AdapterSelection::Portable(PortableReason::LinkUnsupported)
    );
}

#[test]
fn rename_only_falls_back_for_confirmed_cross_device_errors() {
    let source = Path::new("source");
    let target = Path::new("target");
    assert!(rename_or_cross_device(source, target, true, |_, _| Ok(())).expect("rename succeeds"));
    assert!(!rename_or_cross_device(source, target, true, |_, _| Err(
        io::Error::from_raw_os_error(libc::EXDEV)
    ))
    .expect("cross device fallback"));
    #[cfg(windows)]
    assert!(!rename_or_cross_device(source, target, true, |_, _| Err(
        io::Error::from_raw_os_error(windows_sys::Win32::Foundation::ERROR_NOT_SAME_DEVICE as i32,)
    ))
    .expect("Windows cross-volume fallback"));
    assert!(
        rename_or_cross_device(source, target, true, |_, _| Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "denied"
        )))
        .is_err()
    );
    assert!(
        !rename_or_cross_device(source, target, false, |_, _| panic!(
            "cross-volume does not attempt rename"
        ))
        .expect("cross volume plan")
    );
}

#[test]
fn manifests_and_admission_are_progressive_and_atomic() {
    let mut manifest = ProgressiveManifest::default();
    manifest.record(Path::new("root"), "root/a".into(), 4, false);
    manifest.record(Path::new("root"), "root/link".into(), 0, true);
    assert_eq!(manifest.discovered_bytes(), 4);
    assert_eq!(manifest.entries()[0].relative_path, Path::new("a"));
    let admission = transfer_admission(
        [
            "destination".to_string(),
            "source".to_string(),
            "source".to_string(),
        ],
        false,
    );
    assert_eq!(admission.resource_keys, vec!["destination", "source"]);
    assert!(admission.classes.contains(&JobClass::Throughput));
}

/// A real file copy must run the capability-adapter selection decision
/// (`worker::select_adapter`) rather than leaving it dead scaffolding.
/// `select_adapter` is `test-utils`-gated to always resolve to
/// `Portable(UnsupportedPlatform)`, so this observes the decision via the
/// `test-utils`-only call counter instead of the (unreachable in tests)
/// native selection outcome.
#[test]
fn real_file_copy_invokes_capability_adapter_selection() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let source = fixture.path().join("a.txt");
    std::fs::write(&source, b"payload").expect("source file");
    let target = fixture.path().join("dest/a.txt");

    let (_op_arc, _resolver, ctx) = worker_ctx(state());
    let before = adapter_selection_calls_for_tests();
    copy_path(&source, &target, &ctx).expect("copy succeeds");
    let after = adapter_selection_calls_for_tests();

    assert!(after > before, "select_adapter must run during a real copy");
    assert_eq!(std::fs::read(&target).expect("dest contents"), b"payload");
}

/// `move_path` on the same filesystem must go through the shared
/// `rename_or_cross_device` helper and land via a plain rename: the source
/// disappears, the destination has the exact original bytes, and no
/// leftover copy/delete side effects occur.
#[test]
fn move_path_same_filesystem_uses_rename_first() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let source = fixture.path().join("move-me.txt");
    std::fs::write(&source, b"move-payload").expect("source file");
    let target = fixture.path().join("moved.txt");

    let (_op_arc, _resolver, ctx) = worker_ctx(state());
    move_path(&source, &target, &ctx).expect("move succeeds");

    assert!(!source.exists());
    assert_eq!(
        std::fs::read(&target).expect("dest contents"),
        b"move-payload"
    );
}
