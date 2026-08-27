use std::fs;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use file_explorer_lib::ipc::types::ThumbnailQuality;
use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::thumbnails::provider::{ProviderCapability, ThumbnailProvider};
use file_explorer_lib::thumbnails::scheduler::ThumbnailSubscriber;
use file_explorer_lib::thumbnails::types::{
    ThumbnailCandidate, ThumbnailFingerprint, ThumbnailState,
};
use file_explorer_lib::thumbnails::{
    ThumbnailService, MAX_RESULTS_PER_BATCH, MAX_RESULT_BYTES_PER_BATCH,
};
use tempfile::tempdir;

fn candidate(path: &Path) -> ThumbnailCandidate {
    let metadata = fs::metadata(path).expect("metadata");
    let modified_rfc3339 =
        file_explorer_lib::fs::system_time_to_rfc3339(Some(metadata.modified().expect("modified")))
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
    service.set_emitter(Arc::new(move |batch| {
        received.lock().expect("events").extend(batch)
    }));
    service.request(vec![
        (
            subscriber("ready"),
            candidate(&directory.path().join("image.png")),
        ),
        (
            subscriber("none"),
            candidate(&directory.path().join("unavailable.png")),
        ),
        (
            subscriber("bad"),
            candidate(&directory.path().join("failed.png")),
        ),
    ]);

    *now.lock().expect("clock") = 15;
    service.flush_due();
    assert!(events.lock().expect("events").is_empty());
    *now.lock().expect("clock") = 16;
    let deadline = Instant::now() + Duration::from_secs(1);
    while events.lock().expect("events").len() < 3 {
        service.flush_due();
        assert!(Instant::now() < deadline, "events timed out");
        *now.lock().expect("clock") += 16;
        std::thread::yield_now();
    }
    let events = events.lock().expect("events");
    assert!(events
        .iter()
        .any(|event| event.tab_id == "ready" && event.data_url.is_some()));
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
    service.set_emitter(Arc::new(move |batch| {
        received.lock().expect("batches").push(batch)
    }));
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
    assert_eq!(
        batches.lock().expect("batches")[0].len(),
        MAX_RESULTS_PER_BATCH
    );
}

struct ProgressiveProvider;

impl ThumbnailProvider for ProgressiveProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Fake
    }

    fn generate(
        &self,
        _candidate: &ThumbnailCandidate,
        preview: file_explorer_lib::thumbnails::provider::ThumbnailPreviewCallback,
    ) -> ThumbnailState {
        let state = file_explorer_lib::thumbnails::types::validated_png_data_url("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADElEQVR42mNk+M/wHwAF/gL+ZfGHkAAAAABJRU5ErkJggg==".to_string())
            .expect("valid preview");
        let ThumbnailState::Ready { data_url, .. } = state else {
            unreachable!("validated preview is ready")
        };
        preview(ThumbnailState::Ready {
            data_url: data_url.clone(),
            quality: ThumbnailQuality::Low,
        });
        ThumbnailState::Ready {
            data_url,
            quality: ThumbnailQuality::High,
        }
    }
}

#[test]
fn progressive_provider_emits_low_then_high_quality() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("image.png");
    fs::write(&path, b"data").expect("fixture");
    let now = Arc::new(Mutex::new(0_u64));
    let clock_now = Arc::clone(&now);
    let service = ThumbnailService::new_with_provider_and_clock(
        Arc::new(ProgressiveProvider),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(move || *clock_now.lock().expect("clock")),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&events);
    service.set_emitter(Arc::new(move |batch| {
        received.lock().expect("events").extend(batch)
    }));
    service.request(vec![(subscriber("tab"), candidate(&path))]);
    let deadline = Instant::now() + Duration::from_secs(1);
    while events.lock().expect("events").len() < 2 {
        *now.lock().expect("clock") += 16;
        service.flush_due();
        std::thread::yield_now();
        assert!(Instant::now() < deadline, "provider did not run");
    }
    let qualities = events
        .lock()
        .expect("events")
        .iter()
        .filter_map(|event| event.quality)
        .collect::<Vec<_>>();
    assert_eq!(
        qualities,
        vec![ThumbnailQuality::Low, ThumbnailQuality::High]
    );
}

struct LargeProgressiveProvider {
    started: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl ThumbnailProvider for LargeProgressiveProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Fake
    }

    fn generate(
        &self,
        _candidate: &ThumbnailCandidate,
        preview: file_explorer_lib::thumbnails::provider::ThumbnailPreviewCallback,
    ) -> ThumbnailState {
        let _ = self.started.send(());
        let _ = self.release.lock().expect("release").recv();
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&1_u32.to_be_bytes());
        png.extend_from_slice(&1_u32.to_be_bytes());
        png.resize(200_000, 0);
        let state = file_explorer_lib::thumbnails::types::validated_png_data_url(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
        .expect("valid large preview");
        let ThumbnailState::Ready { data_url, .. } = state else {
            unreachable!("validated preview is ready")
        };
        preview(ThumbnailState::Ready {
            data_url: data_url.clone(),
            quality: ThumbnailQuality::Low,
        });
        ThumbnailState::Ready {
            data_url,
            quality: ThumbnailQuality::High,
        }
    }
}

#[test]
fn immediate_progressive_results_respect_the_byte_batch_limit() {
    let directory = tempdir().expect("directory");
    let path = directory.path().join("large.png");
    fs::write(&path, b"data").expect("fixture");
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let service = ThumbnailService::new_with_provider_and_clock(
        Arc::new(LargeProgressiveProvider {
            started: started_tx,
            release: Mutex::new(release_rx),
        }),
        Arc::new(ResourceCoordinator::new()),
        Arc::new(|| 0),
    );
    let batches = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&batches);
    service.set_emitter(Arc::new(move |batch| {
        received.lock().expect("batches").push(batch)
    }));
    service.request(vec![
        (subscriber("left"), candidate(&path)),
        (subscriber("right"), candidate(&path)),
    ]);
    started_rx
        .recv_timeout(Duration::from_millis(200))
        .expect("provider started");
    release_tx.send(()).expect("release provider");

    let deadline = Instant::now() + Duration::from_secs(1);
    while batches.lock().expect("batches").len() < 2 {
        assert!(Instant::now() < deadline, "preview batches timed out");
        std::thread::yield_now();
    }
    let batches = batches.lock().expect("batches");
    assert_eq!(batches[0].len(), 1);
    assert_eq!(batches[1].len(), 1);
    assert!(batches[0].iter().chain(&batches[1]).all(|event| {
        event.data_url.as_ref().is_some_and(|data_url| {
            data_url.len() < MAX_RESULT_BYTES_PER_BATCH
                && event.quality == Some(ThumbnailQuality::Low)
        })
    }));
}
