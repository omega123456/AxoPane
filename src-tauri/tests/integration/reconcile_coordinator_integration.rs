//! Phase 5 coverage for bounded watch compaction and lifecycle-safe focus
//! reconciliation. These tests use only in-memory channels and coordinator
//! permits, so they are deterministic on Windows and macOS.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::bounded;
use file_explorer_lib::reconcile::ReconcileCoordinator;
use file_explorer_lib::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};
use file_explorer_lib::watch::coordinator::{
    CompactedBatch, ControlMessage, MutationKind, RawMutation, WatchCoordinator, WatchId,
    MAX_PENDING_MUTATIONS_PER_WATCH,
};

const PROMPT: Duration = Duration::from_millis(800);

#[test]
fn continuous_mutations_compact_to_one_bounded_dirty_resnapshot() {
    let (batches_tx, batches_rx) = bounded(4);
    let coordinator = WatchCoordinator::spawn(Arc::new(move |batch| {
        batches_tx.send(batch).expect("test receives batch");
    }));
    let raw = coordinator.raw_sender();
    let watch_id = WatchId(7);

    for index in 0..=MAX_PENDING_MUTATIONS_PER_WATCH {
        raw.push(RawMutation {
            watch_id,
            child_path: PathBuf::from(format!("/bounded/{index}")),
            kind: MutationKind::Changed,
        });
    }

    match batches_rx
        .recv_timeout(PROMPT)
        .expect("dirty batch arrives")
    {
        CompactedBatch::Dirty {
            watch_id: actual, ..
        } => assert_eq!(actual, watch_id),
        other => panic!("expected dirty resnapshot, got {other:?}"),
    }
    coordinator.shutdown();
}

#[test]
fn priority_control_is_not_starved_by_raw_mutation_saturation() {
    let (batches_tx, batches_rx) = bounded(4);
    let coordinator = WatchCoordinator::spawn(Arc::new(move |batch| {
        batches_tx.send(batch).expect("test receives batch");
    }));
    let raw = coordinator.raw_sender();
    let watch_id = WatchId(8);

    for index in 0..5000 {
        raw.push(RawMutation {
            watch_id,
            child_path: PathBuf::from(format!("/storm/{index}")),
            kind: MutationKind::Changed,
        });
    }
    assert!(coordinator.send_control(ControlMessage::ForceResnapshot(watch_id)));

    let deadline = Instant::now() + PROMPT;
    let mut saw_dirty = false;
    while Instant::now() < deadline {
        match batches_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(CompactedBatch::Dirty {
                watch_id: actual, ..
            }) if actual == watch_id => {
                saw_dirty = true;
                break;
            }
            Ok(_) | Err(_) => {}
        }
    }
    assert!(
        saw_dirty,
        "priority resnapshot completed under raw saturation"
    );
    coordinator.shutdown();
}

#[test]
fn focus_requests_return_immediately_and_only_latest_generation_publishes() {
    let resources = Arc::new(ResourceCoordinator::new());
    let holds = (0..4)
        .map(|index| {
            resources
                .submit(JobSpec::new([JobClass::Latency], [format!("hold-{index}")]))
                .expect("hold admitted")
        })
        .collect::<Vec<_>>();
    let reconcile = Arc::new(ReconcileCoordinator::default());
    let published = Arc::new(Mutex::new(Vec::new()));
    let start = Instant::now();
    let first = reconcile.request_reconcile(Arc::clone(&resources), "focus-volume".to_string(), {
        let published = Arc::clone(&published);
        move |generation| published.lock().expect("published lock").push(generation)
    });
    let second = reconcile.request_reconcile(Arc::clone(&resources), "focus-volume".to_string(), {
        let published = Arc::clone(&published);
        move |generation| published.lock().expect("published lock").push(generation)
    });
    assert!(start.elapsed() < Duration::from_millis(100));
    assert!(second > first);

    drop(holds);
    let deadline = Instant::now() + PROMPT;
    while Instant::now() < deadline && published.lock().expect("published lock").is_empty() {
        std::thread::yield_now();
    }
    assert_eq!(*published.lock().expect("published lock"), vec![second]);
    resources.shutdown();
}

#[test]
fn bump_generation_advances_per_target_and_is_current_tracks_the_latest() {
    let reconcile = ReconcileCoordinator::default();

    assert_eq!(reconcile.bump_generation("focus-tree"), 1);
    assert_eq!(reconcile.bump_generation("focus-tree"), 2);
    assert!(reconcile.is_current("focus-tree", 2));
    assert!(!reconcile.is_current("focus-tree", 1));

    // A distinct target coalesces independently and starts from its own zero.
    assert_eq!(reconcile.bump_generation("focus-volume"), 1);
    assert!(reconcile.is_current("focus-volume", 1));
    assert!(reconcile.is_current("focus-tree", 2));
}

#[test]
fn generation_for_tests_reports_the_reserved_test_targets_generation() {
    let reconcile = ReconcileCoordinator::default();
    assert_eq!(
        file_explorer_lib::reconcile::generation_for_tests(&reconcile),
        0
    );
    reconcile.bump_generation("test");
    reconcile.bump_generation("test");
    assert_eq!(
        file_explorer_lib::reconcile::generation_for_tests(&reconcile),
        2
    );
}

#[test]
fn distinct_focus_targets_each_run_when_requested_by_the_same_focus_event() {
    let resources = Arc::new(ResourceCoordinator::new());
    let reconcile = Arc::new(ReconcileCoordinator::default());
    let (published_tx, published_rx) = bounded(2);

    for target in ["volume-registry", "watch-service"] {
        let published_tx = published_tx.clone();
        reconcile.request_reconcile(Arc::clone(&resources), target.to_string(), move |_| {
            published_tx.send(target).expect("test receives target");
        });
    }

    let mut published = vec![
        published_rx
            .recv_timeout(PROMPT)
            .expect("first target runs"),
        published_rx
            .recv_timeout(PROMPT)
            .expect("second target runs"),
    ];
    published.sort_unstable();
    assert_eq!(published, ["volume-registry", "watch-service"]);
    resources.shutdown();
}
