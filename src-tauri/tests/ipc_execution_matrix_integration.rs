//! Execution-matrix regression coverage. These assertions exercise owned,
//! in-memory executors only; `test-utils` never reaches machine-global APIs.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::ipc::executor::{Cancellation, IpcExecutor};
use file_explorer_lib::reconcile::ReconcileCoordinator;
use file_explorer_lib::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

const HEAVY_MATRIX: &[(&str, &str)] = &[
    ("directory session/range/release", "DirectorySessionService"),
    ("tree/filesystem metadata", "IpcExecutor latency"),
    ("create/rename/open/clipboard", "IpcExecutor latency"),
    ("watch control", "WatchService control lane"),
    ("focus reconcile", "ReconcileCoordinator"),
    ("volume list/refresh/eject", "Registry snapshot/IpcExecutor"),
    (
        "count/size/icon metadata",
        "ItemCountService/SizeService/IpcExecutor",
    ),
    ("copy/move/delete/archive/extract", "OpsService"),
    ("trash", "IpcExecutor latency"),
    ("native menus", "NativeMenuService helper/IpcExecutor"),
    (
        "app picker/default application",
        "IpcExecutor app-picker LaunchServices owner",
    ),
    ("persistence/log reads", "persistence worker/IpcExecutor"),
];

#[test]
fn every_heavy_matrix_family_has_exactly_one_named_owner() {
    let mut families = std::collections::BTreeSet::new();
    for (family, owner) in HEAVY_MATRIX {
        assert!(
            families.insert(*family),
            "duplicate matrix family: {family}"
        );
        assert!(!owner.is_empty(), "matrix family has no owner: {family}");
    }
    assert_eq!(
        families.len(),
        12,
        "all plan matrix families are enumerated"
    );
}

#[test]
fn fixed_ipc_queue_rejects_overload_without_spawning_more_workers() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = Arc::new(IpcExecutor::new(Arc::clone(&coordinator)));
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = crossbeam_channel::bounded(4);
    let started = Arc::new(AtomicUsize::new(0));
    let mut callers = Vec::new();

    // Occupy every fixed worker first. Each task waits on a channel rather
    // than a delay, so the queue state is deterministic on every platform.
    for index in 0..4 {
        let executor = Arc::clone(&executor);
        let started = Arc::clone(&started);
        let started_tx = started_tx.clone();
        let release_rx = release_rx.clone();
        callers.push(thread::spawn(move || {
            tauri::async_runtime::block_on(executor.latency(format!("active-{index}"), move || {
                started.fetch_add(1, Ordering::SeqCst);
                started_tx.send(()).expect("test start receiver");
                release_rx.recv().expect("test releases worker");
                Ok(())
            }))
        }));
    }
    for _ in 0..4 {
        started_rx.recv().expect("all fixed workers started");
    }
    assert_eq!(started.load(Ordering::SeqCst), 4);

    for index in 0..32 {
        let executor = Arc::clone(&executor);
        callers.push(thread::spawn(move || {
            tauri::async_runtime::block_on(executor.latency(format!("queued-{index}"), || Ok(())))
        }));
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    while executor.queued_len() != 32 {
        assert!(
            Instant::now() < deadline,
            "IPC queue did not reach its fixed capacity"
        );
        thread::yield_now();
    }

    let overload = tauri::async_runtime::block_on(executor.latency("overflow".into(), || Ok(())));
    assert_eq!(overload.unwrap_err(), "IPC latency queue is full");

    for _ in 0..4 {
        release_tx.send(()).expect("release fixed worker");
    }
    for caller in callers {
        caller
            .join()
            .expect("IPC caller thread")
            .expect("queued IPC work");
    }
    executor.shutdown();
    coordinator.shutdown();
}

#[test]
fn focus_reconcile_acknowledges_before_its_latency_executor_runs() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let held = (0..4)
        .map(|index| {
            coordinator
                .submit(JobSpec::new([JobClass::Latency], [format!("hold-{index}")]))
                .expect("test latency permit")
        })
        .collect::<Vec<_>>();
    let reconcile = Arc::new(ReconcileCoordinator::default());
    let generation = reconcile.request_reconcile(Arc::clone(&coordinator), "volume".into(), |_| {});
    assert!(
        generation > 0,
        "matrix row: focus reconcile returns an acknowledgement"
    );
    drop(held);
    coordinator.shutdown();
}

#[test]
fn resource_executor_shutdown_rejects_new_owned_work() {
    let coordinator = ResourceCoordinator::new();
    coordinator.shutdown();
    let result = coordinator.submit(JobSpec::new(
        [JobClass::Latency],
        ["safe-test-resource".to_string()],
    ));
    assert!(
        result.is_err(),
        "matrix row: shutdown owns cancellation of queued work"
    );
}

#[test]
fn bounded_ipc_executor_uses_shared_latency_admission_and_shutdown_owner() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = IpcExecutor::new(Arc::clone(&coordinator));
    let value =
        tauri::async_runtime::block_on(executor.latency("fixture-volume".into(), || Ok(7_u8)))
            .expect("matrix row: bounded filesystem work returns through owner");
    assert_eq!(value, 7);
    executor.shutdown();
    let rejected =
        tauri::async_runtime::block_on(executor.latency("fixture-volume".into(), || Ok(8_u8)));
    assert!(
        rejected.is_err(),
        "matrix row: executor shutdown rejects new work"
    );
    coordinator.shutdown();
}

#[test]
fn saturated_ipc_queue_shutdown_drains_without_losing_worker_termination() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = Arc::new(IpcExecutor::new(Arc::clone(&coordinator)));
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = crossbeam_channel::bounded(4);
    let mut active = Vec::new();

    for index in 0..4 {
        let executor = Arc::clone(&executor);
        let started_tx = started_tx.clone();
        let release_rx = release_rx.clone();
        active.push(thread::spawn(move || {
            tauri::async_runtime::block_on(executor.latency(format!("active-{index}"), move || {
                started_tx.send(()).expect("active worker started");
                release_rx.recv().expect("release active worker");
                Ok(())
            }))
        }));
    }
    for _ in 0..4 {
        started_rx.recv().expect("all workers occupied");
    }

    let mut queued = Vec::new();
    for index in 0..32 {
        let executor = Arc::clone(&executor);
        queued.push(thread::spawn(move || {
            tauri::async_runtime::block_on(executor.latency(format!("queued-{index}"), || Ok(())))
        }));
    }
    let deadline = Instant::now() + Duration::from_millis(500);
    while executor.queued_len() != 32 {
        assert!(Instant::now() < deadline, "queue did not saturate");
        thread::yield_now();
    }

    let (shutdown_done_tx, shutdown_done_rx) = mpsc::channel();
    let shutdown_executor = Arc::clone(&executor);
    thread::spawn(move || {
        shutdown_executor.shutdown();
        shutdown_done_tx
            .send(())
            .expect("shutdown completion receiver");
    });
    let deadline = Instant::now() + Duration::from_millis(500);
    while !executor.is_shutting_down() {
        assert!(
            Instant::now() < deadline,
            "shutdown did not cross the admission boundary"
        );
        thread::yield_now();
    }
    for _ in 0..4 {
        release_tx.send(()).expect("release active worker");
    }

    shutdown_done_rx
        .recv_timeout(Duration::from_millis(750))
        .expect("shutdown must not wait for a full queue to accept sentinels");
    for caller in active {
        caller
            .join()
            .expect("active caller thread")
            .expect("active work");
    }
    for caller in queued {
        assert!(caller.join().expect("queued caller thread").is_err());
    }
    coordinator.shutdown();
}

#[test]
fn cancelled_ipc_job_never_enters_its_platform_or_filesystem_closure() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = IpcExecutor::new(Arc::clone(&coordinator));
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let result = tauri::async_runtime::block_on(executor.latency_cancellable(
        "safe-test-resource".into(),
        cancellation,
        |_| -> Result<(), String> { panic!("cancelled matrix work must not execute") },
    ));
    assert!(
        result.is_err(),
        "matrix row: cancellation is owned before execution"
    );
    executor.shutdown();
    coordinator.shutdown();
}

#[test]
fn owned_ipc_work_honours_its_explicit_completion_deadline() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = IpcExecutor::new(Arc::clone(&coordinator));
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = crossbeam_channel::bounded::<()>(1);

    let result = tauri::async_runtime::block_on(executor.latency_cancellable_with_deadline(
        "safe-test-resource".into(),
        Cancellation::default(),
        Duration::from_millis(25),
        move |_| {
            started_tx.send(()).expect("owned work started");
            release_rx.recv().expect("test release");
            Ok(())
        },
    ));

    started_rx
        .recv_timeout(Duration::from_millis(100))
        .expect("owned work must start before its caller deadline");
    assert_eq!(
        result.unwrap_err(),
        "IPC latency request exceeded its deadline"
    );
    release_tx.send(()).expect("release timed-out work");
    executor.shutdown();
    coordinator.shutdown();
}

#[test]
fn owned_ipc_executor_runs_async_platform_work_through_its_bounded_owner() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = IpcExecutor::new(Arc::clone(&coordinator));

    let value = tauri::async_runtime::block_on(executor.latency_async_cancellable(
        "safe-test-resource".into(),
        Cancellation::default(),
        |cancellation| async move {
            assert!(
                !cancellation.is_cancelled(),
                "the owned async request must start with a live cancellation token"
            );
            Ok(9_u8)
        },
    ))
    .expect("owned async work returns through the executor");

    assert_eq!(value, 9);
    executor.shutdown();
    coordinator.shutdown();
}
