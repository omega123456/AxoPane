//! Execution-matrix regression coverage. These assertions exercise owned,
//! in-memory executors only; `test-utils` never reaches machine-global APIs.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::common::wait_until;
use file_explorer_lib::ipc::executor::{Cancellation, IpcExecutor, LatencyRequest};
use file_explorer_lib::reconcile::{ReconcileCoordinator, ReconcileResource};
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
    (
        "native thumbnails",
        "ThumbnailService/ResourceCoordinator CPU",
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
        13,
        "all plan matrix families are enumerated"
    );
}

#[test]
fn fixed_ipc_queue_rejects_overload_without_spawning_more_workers() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = Arc::new(IpcExecutor::new(Arc::clone(&coordinator)));
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = crossbeam_channel::bounded(4);
    let started = Arc::new(AtomicUsize::new(0));
    let mut callers = Vec::new();

    // Occupy every fixed worker first. Each task waits on a channel rather
    // than a delay, so the queue state is deterministic on every platform.
    for index in 0..4 {
        let executor = Arc::clone(&executor);
        let started = Arc::clone(&started);
        let started_sender = started_sender.clone();
        let release_receiver = release_receiver.clone();
        callers.push(spawn_active_caller(executor, index, move || {
            started.fetch_add(1, Ordering::SeqCst);
            started_sender.send(()).expect("test start receiver");
            release_receiver.recv().expect("test releases worker");
        }));
    }
    for _ in 0..4 {
        started_receiver.recv().expect("all fixed workers started");
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
        release_sender.send(()).expect("release fixed worker");
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
    let generation = reconcile.request_reconcile(
        ReconcileResource {
            coordinator: Arc::clone(&coordinator),
            resource_key: "volume".into(),
        },
        |_| {},
    );
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

const ACTIVE_WORKERS: usize = 4;
const QUEUED_CALLERS: usize = 32;

type CallerHandle = thread::JoinHandle<Result<(), String>>;

fn occupy_every_worker(
    executor: &Arc<IpcExecutor>,
) -> (Vec<CallerHandle>, crossbeam_channel::Sender<()>) {
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = crossbeam_channel::bounded(ACTIVE_WORKERS);
    let mut active = Vec::new();
    for index in 0..ACTIVE_WORKERS {
        let started_sender = started_sender.clone();
        let release_receiver = release_receiver.clone();
        active.push(spawn_active_caller(
            Arc::clone(executor),
            index,
            move || {
                started_sender.send(()).expect("active worker started");
                release_receiver.recv().expect("release active worker");
            },
        ));
    }
    for _ in 0..ACTIVE_WORKERS {
        started_receiver.recv().expect("all workers occupied");
    }
    (active, release_sender)
}

fn saturate_queue(executor: &Arc<IpcExecutor>) -> Vec<CallerHandle> {
    let queued = (0..QUEUED_CALLERS)
        .map(|index| {
            let executor = Arc::clone(executor);
            thread::spawn(move || {
                tauri::async_runtime::block_on(
                    executor.latency(format!("queued-{index}"), || Ok(())),
                )
            })
        })
        .collect();
    wait_until(Duration::from_millis(500), "queue did not saturate", || {
        executor.queued_len() == QUEUED_CALLERS
    });
    queued
}

fn shutdown_in_background(executor: &Arc<IpcExecutor>) -> mpsc::Receiver<()> {
    let (shutdown_done_sender, shutdown_done_receiver) = mpsc::channel();
    let shutdown_executor = Arc::clone(executor);
    thread::spawn(move || {
        shutdown_executor.shutdown();
        shutdown_done_sender
            .send(())
            .expect("shutdown completion receiver");
    });
    wait_until(
        Duration::from_millis(500),
        "shutdown did not cross the admission boundary",
        || executor.is_shutting_down(),
    );
    shutdown_done_receiver
}

#[test]
fn saturated_ipc_queue_shutdown_drains_without_losing_worker_termination() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = Arc::new(IpcExecutor::new(Arc::clone(&coordinator)));
    let (active, release_sender) = occupy_every_worker(&executor);
    let queued = saturate_queue(&executor);

    let shutdown_done_receiver = shutdown_in_background(&executor);
    for _ in 0..ACTIVE_WORKERS {
        release_sender.send(()).expect("release active worker");
    }

    shutdown_done_receiver
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
        LatencyRequest::cancellable("safe-test-resource".into(), cancellation),
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
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = crossbeam_channel::bounded::<()>(1);

    let result = tauri::async_runtime::block_on(executor.latency_cancellable(
        LatencyRequest::new("safe-test-resource".into()).with_deadline(Duration::from_millis(25)),
        move |_| {
            started_sender.send(()).expect("owned work started");
            release_receiver.recv().expect("test release");
            Ok(())
        },
    ));

    started_receiver
        .recv_timeout(Duration::from_millis(100))
        .expect("owned work must start before its caller deadline");
    assert_eq!(
        result.unwrap_err(),
        "IPC latency request exceeded its deadline"
    );
    release_sender.send(()).expect("release timed-out work");
    executor.shutdown();
    coordinator.shutdown();
}

#[test]
fn owned_ipc_executor_runs_async_platform_work_through_its_bounded_owner() {
    let coordinator = Arc::new(ResourceCoordinator::new());
    let executor = IpcExecutor::new(Arc::clone(&coordinator));

    let value = tauri::async_runtime::block_on(executor.latency_async_cancellable(
        LatencyRequest::new("safe-test-resource".into()),
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

fn spawn_active_caller(
    executor: Arc<IpcExecutor>,
    index: usize,
    work: impl FnOnce() + Send + 'static,
) -> thread::JoinHandle<Result<(), String>> {
    thread::spawn(move || {
        tauri::async_runtime::block_on(executor.latency(format!("active-{index}"), move || {
            work();
            Ok(())
        }))
    })
}
