use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::thumbnails::scheduler::{
    ThumbnailScheduler, ThumbnailSubscriber, MAX_ACTIVE_JOBS, MAX_QUEUED_CANDIDATES,
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
fn limits_are_fixed_and_scope_cancellation_is_safe() {
    assert_eq!(MAX_ACTIVE_JOBS, 2);
    assert_eq!(MAX_QUEUED_CANDIDATES, 64);
    let scheduler = ThumbnailScheduler::new(
        file_explorer_lib::thumbnails::platform_provider().into(),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(|_, _: ThumbnailState, _| {}),
    );
    scheduler.submit(subscriber("old", 1), candidate("cancelled.png"));
    scheduler.cancel_scope("left", "old", "/folder", 1);
    let (active, queued) = scheduler.counts();
    assert!(active <= MAX_ACTIVE_JOBS && queued <= MAX_QUEUED_CANDIDATES);
    scheduler.shutdown();
}
