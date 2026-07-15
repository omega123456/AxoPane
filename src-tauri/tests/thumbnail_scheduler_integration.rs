use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use file_explorer_lib::ipc::types::ThumbnailPriority;
use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::thumbnails::provider::{
    ProviderCapability, ThumbnailPreviewCallback, ThumbnailProvider,
};
use file_explorer_lib::thumbnails::scheduler::{
    ThumbnailScheduler, ThumbnailSubscriber, MAX_ACTIVE_JOBS, MAX_DESIRED_PER_SCOPE,
};
use file_explorer_lib::thumbnails::types::{
    ThumbnailCandidate, ThumbnailFingerprint, ThumbnailState,
};

fn candidate(name: &str) -> ThumbnailCandidate {
    ThumbnailCandidate::new(
        ThumbnailFingerprint::from_metadata(Path::new(name), 1, 2),
        false,
    )
}
fn subscriber(tab: &str, generation: u64) -> ThumbnailSubscriber {
    ThumbnailSubscriber {
        pane_id: "left".into(),
        tab_id: tab.into(),
        path: "/folder".into(),
        generation,
    }
}

struct GateProvider {
    started: mpsc::Sender<String>,
    unblock: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl ThumbnailProvider for GateProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Fake
    }

    fn generate(
        &self,
        candidate: &ThumbnailCandidate,
        _preview: ThumbnailPreviewCallback,
    ) -> ThumbnailState {
        let _ = self
            .started
            .send(candidate.fingerprint.path.to_string_lossy().into_owned());
        let _ = self.release.lock().expect("release").recv();
        ThumbnailState::Unavailable
    }

    fn shutdown(&self) {
        for _ in 0..MAX_ACTIVE_JOBS {
            let _ = self.unblock.send(());
        }
    }
}

#[test]
fn shares_work_and_preserves_live_subscriber_context() {
    let results = Arc::new(std::sync::Mutex::new(Vec::new()));
    let received = Arc::clone(&results);
    let scheduler = ThumbnailScheduler::new(
        file_explorer_lib::thumbnails::platform_provider().into(),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(move |_, _, subscribers| received.lock().expect("results").push(subscribers)),
    );
    scheduler.submit(subscriber("a", 1), candidate("shared.png"));
    scheduler.submit(subscriber("b", 1), candidate("shared.png"));
    let deadline = Instant::now() + Duration::from_secs(1);
    while results.lock().expect("results").is_empty() {
        assert!(Instant::now() < deadline, "completion timed out");
        std::thread::yield_now();
    }
    assert_eq!(results.lock().expect("results")[0].len(), 2);
}

#[test]
fn verifies_metadata_once_per_generated_miss() {
    let checks = Arc::new(AtomicUsize::new(0));
    let checks_for_verifier = Arc::clone(&checks);
    let (completed_tx, completed_rx) = mpsc::channel();
    let scheduler = ThumbnailScheduler::new_with_metadata_verifier(
        file_explorer_lib::thumbnails::platform_provider().into(),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(move |_, _, _| {
            let _ = completed_tx.send(());
        }),
        Arc::new(move |_| {
            checks_for_verifier.fetch_add(1, Ordering::Relaxed);
            true
        }),
    );
    scheduler.submit(subscriber("metadata", 1), candidate("metadata.png"));
    completed_rx
        .recv_timeout(Duration::from_millis(200))
        .expect("thumbnail completed");
    assert_eq!(checks.load(Ordering::Relaxed), 1);
}

#[test]
fn limits_are_fixed_and_scope_cancellation_is_safe() {
    assert_eq!(MAX_ACTIVE_JOBS, 2);
    assert_eq!(MAX_DESIRED_PER_SCOPE, 4_096);
    let scheduler = ThumbnailScheduler::new(
        file_explorer_lib::thumbnails::platform_provider().into(),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(|_, _: ThumbnailState, _| {}),
    );
    scheduler.submit(subscriber("old", 1), candidate("cancelled.png"));
    scheduler.cancel_scope("left", "old", "/folder", 1);
    let (active, queued) = scheduler.counts();
    assert!(active <= MAX_ACTIVE_JOBS && queued <= MAX_DESIRED_PER_SCOPE);
    scheduler.shutdown();
}

#[test]
fn replacement_retains_ranges_larger_than_the_old_queue_limit() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scheduler = ThumbnailScheduler::new(
        Arc::new(GateProvider {
            started: started_tx,
            unblock: release_tx.clone(),
            release: Mutex::new(release_rx),
        }),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(|_, _, _| {}),
    );
    scheduler.submit(subscriber("block-a", 1), candidate("block-a.png"));
    scheduler.submit(subscriber("block-b", 1), candidate("block-b.png"));
    for _ in 0..2 {
        started_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("blocker started");
    }

    let requested = (0..144)
        .map(|index| {
            (
                candidate(&format!("preview-{index}.png")),
                ThumbnailPriority::Visible,
                index,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        scheduler.replace_scope(subscriber("range", 1), 1, requested),
        144
    );
    assert_eq!(scheduler.counts(), (2, 144));

    scheduler.cancel_scope("left", "range", "/folder", 1);
    release_tx.send(()).expect("release first");
    release_tx.send(()).expect("release second");
    scheduler.shutdown();
}

#[test]
fn visible_work_precedes_directional_and_behind_prefetch() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scheduler = ThumbnailScheduler::new(
        Arc::new(GateProvider {
            started: started_tx,
            unblock: release_tx.clone(),
            release: Mutex::new(release_rx),
        }),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(|_, _, _| {}),
    );
    let jobs = vec![
        (candidate("behind.png"), ThumbnailPriority::Behind, 0),
        (candidate("visible.png"), ThumbnailPriority::Visible, 10),
        (candidate("ahead.png"), ThumbnailPriority::Ahead, 0),
    ];
    assert_eq!(
        scheduler.replace_scope(subscriber("priority", 1), 1, jobs),
        3
    );
    let started = (0..MAX_ACTIVE_JOBS)
        .map(|_| {
            started_rx
                .recv_timeout(Duration::from_millis(200))
                .expect("priority work started")
        })
        .collect::<Vec<_>>();
    assert!(started.iter().any(|path| path.ends_with("visible.png")));
    assert!(started.iter().any(|path| path.ends_with("ahead.png")));
    scheduler.cancel_scope("left", "priority", "/folder", 1);
    release_tx.send(()).expect("release first");
    release_tx.send(()).expect("release second");
    scheduler.shutdown();
}
