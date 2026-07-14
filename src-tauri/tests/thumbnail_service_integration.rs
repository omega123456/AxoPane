use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::thumbnails::provider::{ProviderCapability, ThumbnailProvider};
use file_explorer_lib::thumbnails::scheduler::ThumbnailSubscriber;
use file_explorer_lib::thumbnails::types::{
    ThumbnailCandidate, ThumbnailFingerprint, ThumbnailState,
};
use file_explorer_lib::thumbnails::{ThumbnailService, MAX_RESULTS_PER_BATCH};
use tempfile::tempdir;

fn candidate(path: &Path) -> ThumbnailCandidate {
    let metadata = fs::metadata(path).expect("metadata");
    let modified_rfc3339 = file_explorer_lib::fs::system_time_to_rfc3339(Some(
        metadata.modified().expect("modified"),
    ))
    .expect("listing timestamp");
    let modified_unix_seconds = u64::try_from(
        OffsetDateTime::parse(&modified_rfc3339, &Rfc3339)
            .expect("parse listing timestamp")
            .unix_timestamp(),
    )
    .expect("post epoch");
    ThumbnailCandidate::new(
        ThumbnailFingerprint::from_metadata(path, modified_unix_seconds, metadata.len()),
        false,
    )
}

fn subscriber(tab: &str) -> ThumbnailSubscriber {
    ThumbnailSubscriber {
        pane_id: "left".into(),
        tab_id: tab.into(),
        path: "/folder".into(),
        generation: 7,
    }
}

#[test]
fn fake_provider_emits_contextual_ready_and_negative_results_in_a_timed_batch() {
    let directory = tempdir().expect("directory");
    for name in ["image.png", "unavailable.png", "failed.png"] {
        fs::write(directory.path().join(name), b"data").expect("fixture");
    }
    let now = Arc::new(Mutex::new(0_u64));
    let clock_now = Arc::clone(&now);
    let service = ThumbnailService::new_with_provider_and_clock(
        file_explorer_lib::thumbnails::platform_provider().into(),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(move || *clock_now.lock().expect("clock")),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&events);
    service.set_emitter(Arc::new(move |batch| received.lock().expect("events").extend(batch)));
    service.request(vec![
        (subscriber("ready"), candidate(&directory.path().join("image.png"))),
        (subscriber("none"), candidate(&directory.path().join("unavailable.png"))),
        (subscriber("bad"), candidate(&directory.path().join("failed.png"))),
    ]);

    *now.lock().expect("clock") = 49;
    service.flush_due();
    assert!(events.lock().expect("events").is_empty());
    *now.lock().expect("clock") = 50;
    let deadline = Instant::now() + Duration::from_secs(1);
    while events.lock().expect("events").len() < 3 {
        service.flush_due();
        assert!(Instant::now() < deadline, "events timed out");
        *now.lock().expect("clock") += 50;
        std::thread::yield_now();
    }
    let events = events.lock().expect("events");
    assert!(events.iter().any(|event| event.tab_id == "ready" && event.data_url.is_some()));
    assert!(events.iter().any(|event| event.tab_id == "none"));
    assert!(events.iter().any(|event| event.tab_id == "bad"));
}

#[test]
fn eight_results_flush_immediately_without_advancing_the_clock() {
    let directory = tempdir().expect("directory");
    let now = Arc::new(Mutex::new(0_u64));
    let clock_now = Arc::clone(&now);
    let service = ThumbnailService::new_with_provider_and_clock(
        file_explorer_lib::thumbnails::platform_provider().into(),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(move || *clock_now.lock().expect("clock")),
    );
    let batches = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&batches);
    service.set_emitter(Arc::new(move |batch| received.lock().expect("batches").push(batch)));
    let requests = (0..MAX_RESULTS_PER_BATCH)
        .map(|index| {
            let path = directory.path().join(format!("image-{index}.png"));
            fs::write(&path, b"data").expect("fixture");
            (subscriber(&index.to_string()), candidate(&path))
        })
        .collect();
    service.request(requests);
    let deadline = Instant::now() + Duration::from_secs(1);
    while batches.lock().expect("batches").is_empty() {
        assert!(Instant::now() < deadline, "batch timed out");
        std::thread::yield_now();
    }
    assert_eq!(batches.lock().expect("batches")[0].len(), MAX_RESULTS_PER_BATCH);
}

struct MutatingProvider;

impl ThumbnailProvider for MutatingProvider {
    fn capability(&self) -> ProviderCapability { ProviderCapability::Fake }

    fn generate(&self, candidate: &ThumbnailCandidate) -> ThumbnailState {
        fs::write(&candidate.fingerprint.path, b"changed-size").expect("mutate fixture");
        ThumbnailState::Unavailable
    }
}

#[test]
fn post_generation_metadata_mismatch_is_superseded_without_cache_or_event() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("image.png");
    fs::write(&path, b"data").expect("fixture");
    let service = ThumbnailService::new_with_provider_and_clock(
        Arc::new(MutatingProvider),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(|| 0),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&events);
    service.set_emitter(Arc::new(move |batch| received.lock().expect("events").extend(batch)));
    service.request(vec![(subscriber("tab"), candidate(&path))]);
    let deadline = Instant::now() + Duration::from_secs(1);
    while fs::metadata(&path).expect("metadata").len() == 4 {
        std::thread::yield_now();
        assert!(Instant::now() < deadline, "provider did not run");
    }
    service.flush_due();
    assert_eq!(service.cache_len(), 0);
    assert!(events.lock().expect("events").is_empty());
}
