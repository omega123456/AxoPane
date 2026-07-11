// Whitebox integration tests for Phase 1 of the Tier-1 performance plan:
// throttling single-file transfer progress and capping `OpProgress.item_names`.
//
// These tests need access to private items (`TransferThrottle`, the throttled
// emit helpers, `compress_item_with_progress`, `extract_item_with_progress`),
// so — matching the existing whitebox convention in
// `ops_private_integration.rs` — this file `include!`s the module source
// directly instead of depending on the crate's public surface.

include!("../src/ops/mod.rs");

mod volumes {
    pub use file_explorer_lib::volumes::*;
}

// The source-included ops module uses the production crate-root traversal
// path. Mirror that export for this whitebox integration crate.
mod traversal {
    pub use file_explorer_lib::traversal::*;
}

mod resource_coordinator {
    pub use file_explorer_lib::resource_coordinator::*;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn item(path: &Path, size_bytes: u64) -> OpItem {
        OpItem {
            source_path: path.to_string_lossy().into_owned(),
            name: path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned(),
            size_bytes,
        }
    }

    fn op_state_with_items(status: OpStatus, items: Vec<OpItem>) -> OpState {
        let total_items = items.len() as u64;
        OpState {
            id: "op-1".to_string(),
            kind: OpKind::Copy,
            destination_dir: PathBuf::from("dest"),
            items,
            volumes: HashSet::from(["/".to_string()]),
            status,
            total_items,
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

    /// A manually steppable clock: the test sets the current instant directly
    /// (rather than relying on real elapsed wall-clock time), so throttle
    /// spacing assertions are fully deterministic and fast.
    fn controllable_clock() -> (Arc<Mutex<Instant>>, InstantNow) {
        let now = Arc::new(Mutex::new(Instant::now()));
        let now_for_closure = now.clone();
        let closure: InstantNow = Arc::new(move || *now_for_closure.lock().expect("clock lock"));
        (now, closure)
    }

    type OpCtx = (
        Arc<Mutex<OpState>>,
        Arc<Condvar>,
        Arc<Mutex<Vec<OpProgress>>>,
        WorkerCtx<'static>,
    );

    fn make_ctx_with_clock(state: OpState, instant_now: InstantNow) -> OpCtx {
        let op_arc = Arc::new(Mutex::new(state));
        let resolver = Arc::new(Condvar::new());
        let progress_log = Arc::new(Mutex::new(Vec::<OpProgress>::new()));
        let progress_for_emitter = progress_log.clone();
        let progress: Option<Arc<dyn Fn(OpProgress) + Send + Sync>> =
            Some(Arc::new(move |progress| {
                progress_for_emitter
                    .lock()
                    .expect("progress log lock")
                    .push(progress);
            }));

        let op_arc_ref = Box::leak(Box::new(op_arc.clone()));
        let resolver_ref = Box::leak(Box::new(resolver.clone()));
        let progress_ref = Box::leak(Box::new(progress));
        let instant_now_ref = Box::leak(Box::new(instant_now));

        let ctx = WorkerCtx {
            op_arc: op_arc_ref,
            resolver: resolver_ref,
            progress: progress_ref,
            start: Instant::now() - Duration::from_secs(1),
            rate_window: Duration::ZERO,
            instant_now: instant_now_ref,
        };

        (op_arc, resolver, progress_log, ctx)
    }

    /// `OpProgress.item_names` is capped to the first two names regardless of
    /// how many top-level items the operation has; `total_items` (which the
    /// UI's "+K more" count reads) still reflects the full count.
    #[test]
    fn op_progress_caps_item_names_to_a_two_item_preview() {
        let items = vec![
            OpItem {
                source_path: "/tmp/a.txt".to_string(),
                name: "a.txt".to_string(),
                size_bytes: 1,
            },
            OpItem {
                source_path: "/tmp/b.txt".to_string(),
                name: "b.txt".to_string(),
                size_bytes: 1,
            },
            OpItem {
                source_path: "/tmp/c.txt".to_string(),
                name: "c.txt".to_string(),
                size_bytes: 1,
            },
            OpItem {
                source_path: "/tmp/d.txt".to_string(),
                name: "d.txt".to_string(),
                size_bytes: 1,
            },
        ];
        let state = op_state_with_items(OpStatus::Active, items);
        let progress = state.progress();

        assert_eq!(
            progress.item_names,
            vec!["a.txt".to_string(), "b.txt".to_string()]
        );
        // The "+K more" count comes from `total_items`, which stays the full
        // top-level item count even though `item_names` is now a preview.
        assert_eq!(progress.total_items, 4);
    }

    /// A single top-level item still reports its one name (no truncation
    /// artifact for the common case).
    #[test]
    fn op_progress_keeps_a_single_item_name_untouched() {
        let state = op_state_with_items(
            OpStatus::Active,
            vec![OpItem {
                source_path: "/tmp/solo.txt".to_string(),
                name: "solo.txt".to_string(),
                size_bytes: 1,
            }],
        );
        let progress = state.progress();
        assert_eq!(progress.item_names, vec!["solo.txt".to_string()]);
        assert_eq!(progress.total_items, 1);
    }

    /// Directly exercises the throttle guard (`maybe_emit_transfer`, driven
    /// through `begin_file_progress` / `report_chunk_progress` /
    /// `finish_file_progress`) with a manually stepped clock: emissions spaced
    /// under the 90 ms interval are swallowed; once the interval elapses, the
    /// next call emits. This is the same throttle now shared by the
    /// single-file transfer path (Phase 1 threads it through
    /// `copy_path_with_total_discovery`'s non-directory branch).
    #[test]
    fn single_file_transfer_throttle_spaces_emissions_by_the_interval() {
        let (clock, instant_now) = controllable_clock();
        let base = *clock.lock().expect("clock lock");
        let (_op_arc, _resolver, progress_log, ctx) = make_ctx_with_clock(
            op_state_with_items(OpStatus::Active, vec![item(Path::new("/tmp/big.bin"), 30)]),
            instant_now,
        );

        let mut throttle = TransferThrottle::new(base, PROGRESS_EMIT_INTERVAL);

        // t=0: begin fires at the same instant the throttle was created, so it
        // is swallowed (0ms elapsed < 90ms interval).
        begin_file_progress(&ctx, Path::new("/tmp/big.bin"), 30, Some(&mut throttle));
        assert!(
            progress_log.lock().expect("progress log lock").is_empty(),
            "begin at t=0 should be throttled"
        );

        // t=50ms: still under the interval, swallowed.
        *clock.lock().expect("clock lock") = base + Duration::from_millis(50);
        report_chunk_progress(&ctx, 10, Some(&mut throttle));
        assert!(
            progress_log.lock().expect("progress log lock").is_empty(),
            "chunk at t=50ms should still be throttled"
        );

        // t=100ms: interval elapsed (>= 90ms since last_emit at t=0), emits.
        *clock.lock().expect("clock lock") = base + Duration::from_millis(100);
        report_chunk_progress(&ctx, 10, Some(&mut throttle));
        {
            let log = progress_log.lock().expect("progress log lock");
            assert_eq!(log.len(), 1, "chunk at t=100ms should emit exactly once");
        }

        // t=150ms: only 50ms since the last emit (t=100ms) — throttled again.
        *clock.lock().expect("clock lock") = base + Duration::from_millis(150);
        report_chunk_progress(&ctx, 10, Some(&mut throttle));
        assert_eq!(
            progress_log.lock().expect("progress log lock").len(),
            1,
            "chunk at t=150ms should still be throttled (only 50ms since last emit)"
        );

        // t=300ms: interval elapsed again, `finish_file_progress` emits.
        *clock.lock().expect("clock lock") = base + Duration::from_millis(300);
        finish_file_progress(&ctx, Some(&mut throttle));
        assert_eq!(
            progress_log.lock().expect("progress log lock").len(),
            2,
            "finish at t=300ms should emit"
        );

        // The operation terminal snapshot is unconditional: `advance_item`
        // always emits regardless of the throttle's internal state.
        *clock.lock().expect("clock lock") = base + Duration::from_millis(301);
        let tracked_item = { ctx.op_arc.lock().expect("op lock").items[0].clone() };
        advance_item(&ctx, &tracked_item);
        assert_eq!(
            progress_log.lock().expect("progress log lock").len(),
            3,
            "advance_item's terminal emission is never throttled"
        );
    }

    /// Compressing a many-small-member source tree shares a single throttle
    /// across every member (Phase 1's `compress_item_with_progress` change).
    /// With real (fast) wall-clock timing, the whole operation completes in
    /// well under the 90 ms interval, so the intra-member emit helpers
    /// (`begin_file_progress`/`finish_file_progress`/`report_chunk_progress`)
    /// collectively emit at most once — not twice per member (which an
    /// unthrottled or per-member-fresh-throttle implementation would do for
    /// 6 members: up to 12 intra-member events).
    #[test]
    fn compress_shares_one_throttle_across_many_small_members() {
        let fixture = tempdir().expect("temp dir");
        let source_dir = fixture.path().join("payload");
        std::fs::create_dir_all(&source_dir).expect("source dir");
        const MEMBER_COUNT: usize = 6;
        for index in 0..MEMBER_COUNT {
            std::fs::write(source_dir.join(format!("member-{index}.txt")), b"x")
                .expect("member file");
        }

        let archive_path = fixture.path().join("payload.zip");
        let source_item = item(&source_dir, 0);
        let (_op_arc, _resolver, progress_log, ctx) = make_ctx_with_clock(
            op_state_with_items(OpStatus::Active, vec![source_item.clone()]),
            Arc::new(Instant::now),
        );

        compress_item_with_progress(&source_dir, &archive_path, &source_item, &ctx)
            .expect("compress many small members");

        assert!(archive_path.exists());
        let intra_member_events = progress_log.lock().expect("progress log lock").len();
        assert!(
            intra_member_events <= 1,
            "expected the shared throttle to bound intra-member events to at most 1 \
             for {MEMBER_COUNT} members, got {intra_member_events}"
        );
    }

    /// Extracting a many-small-member archive shares a single throttle across
    /// every archive entry (Phase 1's `extract_item_with_progress` change),
    /// mirroring the compress-side assertion above.
    #[test]
    fn extract_shares_one_throttle_across_many_small_members() {
        let fixture = tempdir().expect("temp dir");
        let source_dir = fixture.path().join("payload");
        std::fs::create_dir_all(&source_dir).expect("source dir");
        const MEMBER_COUNT: usize = 6;
        for index in 0..MEMBER_COUNT {
            std::fs::write(source_dir.join(format!("member-{index}.txt")), b"x")
                .expect("member file");
        }

        let archive_path = fixture.path().join("payload.zip");
        let archive_item = item(&archive_path, 0);
        let (_op_arc, _resolver, compress_log, compress_ctx) = make_ctx_with_clock(
            op_state_with_items(OpStatus::Active, vec![item(&source_dir, 0)]),
            Arc::new(Instant::now),
        );
        compress_item_with_progress(
            &source_dir,
            &archive_path,
            &item(&source_dir, 0),
            &compress_ctx,
        )
        .expect("compress fixture archive");
        drop(compress_log);

        let extract_root = fixture.path().join("extract");
        let (_op_arc, _resolver, progress_log, ctx) = make_ctx_with_clock(
            op_state_with_items(OpStatus::Active, vec![archive_item.clone()]),
            Arc::new(Instant::now),
        );

        extract_item_with_progress(&archive_path, &extract_root, &archive_item, &ctx)
            .expect("extract many small members");

        let intra_member_events = progress_log.lock().expect("progress log lock").len();
        assert!(
            intra_member_events <= 1,
            "expected the shared throttle to bound intra-member events to at most 1 \
             for {MEMBER_COUNT} members, got {intra_member_events}"
        );
    }

    /// `OpState::progress()` no longer clones every top-level item name: this
    /// is a behavioral proxy (rather than an allocation-counting test) that
    /// confirms the preview stays capped even when the operation has far more
    /// items than the preview size, so the per-tick clone is bounded.
    #[test]
    fn op_progress_preview_stays_bounded_for_large_selections() {
        let items: Vec<OpItem> = (0..500)
            .map(|index| OpItem {
                source_path: format!("/tmp/item-{index}.txt"),
                name: format!("item-{index}.txt"),
                size_bytes: 1,
            })
            .collect();
        let state = op_state_with_items(OpStatus::Active, items);
        let progress = state.progress();
        assert_eq!(progress.item_names.len(), 2);
        assert_eq!(progress.total_items, 500);
    }
}
