// Integration tests for Phase 3 of the Tier-1 performance plan: batched icon
// emission from `request_icons` (covering both the pool/sequential
// production paths' shared batching primitive and the `test-utils`
// synchronous variant's batched return shape).
//
// Two layers:
//   - Black-box: drive the public, `test-utils`-gated `request_icons`
//     command and assert on the number/shape of batches it returns for
//     various path counts (covers the count-threshold flush end-to-end).
//   - White-box: `include!` the pure, IO-free `IconBatcher` primitive
//     (`src/ipc/icon_batch.rs`, which has no `AppHandle`/thread dependency)
//     to deterministically exercise the time-based flush branch with a
//     manually-stepped clock, matching the existing whitebox convention used
//     for the ops/watch throttle tests.

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{IconStateEvent, RequestIconsRequest};

// Shim so `crate::ipc::types::IconStateEvent` (referenced by the whitebox-
// included `src/ipc/icon_batch.rs` below) resolves to the real library type
// against this test binary's own crate root, instead of failing to resolve —
// mirrors the `mod volumes` shim in
// `ops_single_file_progress_throttle_integration.rs`.
mod ipc {
    pub mod types {
        pub use file_explorer_lib::ipc::types::*;
    }
}

fn paths(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("/tmp/icon-{index}.bin")).collect()
}

/// Resolving zero icons produces zero batches (nothing to flush).
#[test]
fn request_icons_emits_no_batches_for_an_empty_request() {
    let batches = commands::request_icons(RequestIconsRequest { paths: paths(0) });
    assert!(batches.is_empty());
}

/// Resolving fewer icons than the batch size produces exactly one batch
/// (the final remainder flush), not one event per path.
#[test]
fn request_icons_batches_a_small_request_into_a_single_flush() {
    let batches = commands::request_icons(RequestIconsRequest { paths: paths(5) });
    assert_eq!(batches.len(), 1, "expected a single remainder flush, not one event per path");
    assert_eq!(batches[0].len(), 5);
}

/// Resolving exactly one full batch's worth of icons flushes once via the
/// count threshold, with nothing left over for a remainder flush.
#[test]
fn request_icons_flushes_exactly_at_the_batch_boundary() {
    let batches = commands::request_icons(RequestIconsRequest { paths: paths(64) });
    assert_eq!(batches.len(), 1, "64 icons should flush as exactly one full batch");
    assert_eq!(batches[0].len(), 64);
}

/// Resolving N icons emits ceil(N / chunk) batched events, not N — the core
/// Phase 3 acceptance criterion — verified across a request that spans
/// several full batches plus a remainder.
#[test]
fn request_icons_emits_ceil_n_over_chunk_batches_not_n_events() {
    const CHUNK: usize = 64;
    let total = 2 * CHUNK + 2;
    let batches = commands::request_icons(RequestIconsRequest { paths: paths(total) });

    let expected_batch_count = total.div_ceil(CHUNK);
    assert_eq!(batches.len(), expected_batch_count);
    assert_eq!(batches[0].len(), CHUNK);
    assert_eq!(batches[1].len(), CHUNK);
    assert_eq!(batches[2].len(), 2);

    let flattened: Vec<IconStateEvent> = batches.into_iter().flatten().collect();
    assert_eq!(flattened.len(), total);
    assert!(flattened.iter().all(|event| event.icon_data_url.is_none()));
    // Every requested path is represented exactly once across the batches.
    let mut seen: Vec<&str> = flattened.iter().map(|event| event.path.as_str()).collect();
    seen.sort();
    let mut expected: Vec<String> = paths(total);
    expected.sort();
    assert_eq!(seen, expected);
}

/// Whitebox coverage of the pure `IconBatcher` primitive: exercises the
/// time-based flush branch (elapsed >= FLUSH_INTERVAL) with a manually
/// stepped clock, independent of any real sleep, plus the count-threshold
/// branch and both branches of `drain_remainder`.
mod icon_batch_unit {
    include!("../src/ipc/icon_batch.rs");

    fn event(path: &str) -> IconStateEvent {
        IconStateEvent {
            path: path.to_string(),
            icon_data_url: None,
        }
    }

    #[test]
    fn push_flushes_once_the_count_threshold_is_reached() {
        let base = Instant::now();
        let mut batcher = IconBatcher::new(base);

        for index in 0..MAX_BATCH - 1 {
            // Same instant every time: no time-based flush possible.
            let flushed = batcher.push(event(&format!("/tmp/{index}")), base);
            assert!(flushed.is_none(), "should not flush before the count threshold");
        }

        let flushed = batcher.push(event("/tmp/last"), base);
        assert_eq!(
            flushed.map(|batch| batch.len()),
            Some(MAX_BATCH),
            "the {MAX_BATCH}th push should flush the full batch"
        );
    }

    #[test]
    fn push_flushes_once_the_time_interval_elapses_even_with_few_icons() {
        let base = Instant::now();
        let mut batcher = IconBatcher::new(base);

        // Well under the interval: buffered, no flush.
        let flushed = batcher.push(event("/tmp/a"), base + Duration::from_millis(10));
        assert!(flushed.is_none());

        // Interval elapsed (>= FLUSH_INTERVAL since the batcher was created):
        // flushes even though far fewer than MAX_BATCH icons are buffered.
        let flushed = batcher.push(event("/tmp/b"), base + FLUSH_INTERVAL);
        let batch = flushed.expect("time-based flush should have fired");
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn drain_remainder_is_none_when_empty_and_some_when_buffered() {
        let base = Instant::now();
        let mut batcher = IconBatcher::new(base);
        assert!(batcher.drain_remainder().is_none(), "nothing buffered yet");

        // Push one icon without crossing either threshold.
        let flushed = batcher.push(event("/tmp/only"), base);
        assert!(flushed.is_none());

        let remainder = batcher.drain_remainder().expect("one buffered icon");
        assert_eq!(remainder.len(), 1);
        assert!(batcher.drain_remainder().is_none(), "buffer was drained");
    }
}
