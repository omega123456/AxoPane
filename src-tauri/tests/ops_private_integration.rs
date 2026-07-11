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
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};
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

    fn op_state(status: OpStatus, volumes: &[&str]) -> OpState {
        OpState {
            id: "op-1".to_string(),
            kind: OpKind::Copy,
            destination_dir: PathBuf::from("dest"),
            items: vec![OpItem {
                source_path: "/tmp/source.txt".to_string(),
                name: "source.txt".to_string(),
                size_bytes: 0,
            }],
            volumes: volumes.iter().map(|value| (*value).to_string()).collect(),
            status,
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

    type OpCtx = (
        Arc<Mutex<OpState>>,
        Arc<Condvar>,
        Arc<Mutex<Vec<OpProgress>>>,
        WorkerCtx<'static>,
    );

    fn make_ctx(state: OpState) -> OpCtx {
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
        let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(Instant::now);

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

    /// Like [`make_ctx`], but the injected clock advances by `step` on every
    /// call instead of tracking real wall-clock time. Phase 1's
    /// `TransferThrottle` (shared by `compress_item_with_progress` /
    /// `extract_item_with_progress`) now gates every intra-member progress
    /// emission on `PROGRESS_EMIT_INTERVAL` elapsing since the last emission;
    /// with a real clock a small/fast fixture never crosses that interval, so
    /// tests that need to observe at least one intra-member snapshot (rather
    /// than only the unconditional per-item completion snapshot) must use a
    /// clock that always advances past the interval between calls.
    fn make_ctx_with_ever_advancing_clock(state: OpState, step: Duration) -> OpCtx {
        let now = Arc::new(Mutex::new(Instant::now()));
        let now_for_closure = now.clone();
        let instant_now: Arc<dyn Fn() -> Instant + Send + Sync> = Arc::new(move || {
            let mut guard = now_for_closure.lock().expect("clock lock");
            *guard += step;
            *guard
        });

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

    #[test]
    fn archive_helpers_round_trip_wrapper_archives_and_progress() {
        let fixture = tempdir().expect("temp dir");
        let source_dir = fixture.path().join("payload");
        std::fs::create_dir_all(source_dir.join("nested")).expect("source tree");
        std::fs::write(source_dir.join("nested").join("inside.txt"), b"payload")
            .expect("source file");

        let archive_path = fixture.path().join("archives").join("payload.zip");
        let extract_root = fixture.path().join("extract");
        let source_item = item(&source_dir, 0);
        let archive_item = item(&archive_path, 0);
        // Phase 1 throttles intra-member compress/extract progress
        // (`TransferThrottle`, 90ms interval): a clock that always advances
        // past the interval between calls guarantees at least one emission
        // still carries the 7-byte file's `current_file_total_bytes`, rather
        // than racing a real (fast) clock that never crosses the interval.
        let (_op_arc, _resolver, progress_log, ctx) = make_ctx_with_ever_advancing_clock(
            op_state(OpStatus::Active, &["/"]),
            Duration::from_millis(100),
        );

        compress_item_with_progress(&source_dir, &archive_path, &source_item, &ctx)
            .expect("compress");
        assert!(archive_path.exists());
        assert!(is_zip_archive_path(&archive_path));
        assert_eq!(archive_stem_for_item(&archive_path), "payload");
        assert_eq!(archive_root_name(&source_dir), PathBuf::from("payload"));
        assert_eq!(zip_uncompressed_size(&archive_path).expect("zip size"), 7);

        let file = std::fs::File::open(&archive_path).expect("archive open");
        let mut archive = ZipArchive::new(file).expect("zip archive");
        assert_eq!(
            detect_redundant_wrapper_root(&mut archive, "payload").expect("wrapper root"),
            Some("payload".to_string())
        );
        assert_eq!(
            strip_wrapper_root(Path::new("payload/nested/inside.txt"), Some("payload")),
            Path::new("nested/inside.txt")
        );

        extract_item_with_progress(&archive_path, &extract_root, &archive_item, &ctx)
            .expect("extract");
        assert_eq!(
            std::fs::read(
                extract_root
                    .join("payload")
                    .join("nested")
                    .join("inside.txt")
            )
            .expect("extracted file"),
            b"payload"
        );

        let next_extract_dir = unique_archive_directory_path(&extract_root, "payload");
        assert_eq!(
            next_extract_dir.file_name().expect("file name"),
            "payload (1)"
        );

        let progress = progress_log.lock().expect("progress log lock");
        assert!(!progress.is_empty());
        assert!(progress
            .iter()
            .any(|entry| entry.current_file_total_bytes == 7));
    }

    #[test]
    fn archive_helpers_reject_invalid_inputs_and_empty_paths() {
        let fixture = tempdir().expect("temp dir");
        let source_file = fixture.path().join("plain.txt");
        std::fs::write(&source_file, b"plain").expect("plain file");
        let existing_archive = fixture.path().join("existing.zip");
        std::fs::write(&existing_archive, b"not zip").expect("existing archive");
        let destination_file = fixture.path().join("destination.txt");
        std::fs::write(&destination_file, b"x").expect("destination file");

        let bad_archive_item = item(&source_file, 0);
        let (_op_arc, _resolver, _progress_log, ctx) = make_ctx(op_state(OpStatus::Active, &["/"]));

        assert!(compress_item_with_progress(
            &source_file,
            &fixture.path().join("output.bin"),
            &bad_archive_item,
            &ctx
        )
        .expect_err("invalid extension")
        .contains(".zip"));
        assert!(compress_item_with_progress(
            &fixture.path().join("missing"),
            &fixture.path().join("missing.zip"),
            &bad_archive_item,
            &ctx
        )
        .expect_err("missing source")
        .contains("does not exist"));
        assert!(compress_item_with_progress(
            &source_file,
            &existing_archive,
            &bad_archive_item,
            &ctx
        )
        .expect_err("existing archive")
        .contains("already exists"));

        assert!(extract_item_with_progress(
            &source_file,
            &destination_file,
            &bad_archive_item,
            &ctx
        )
        .expect_err("extract destination file")
        .contains("not a folder"));
        assert!(to_zip_path(Path::new(""))
            .expect_err("empty zip path")
            .contains("empty"));
        assert_eq!(archive_stem_for_item(Path::new("")), "Archive");
        assert_eq!(archive_root_name(Path::new("")), PathBuf::from("Archive"));
    }

    #[test]
    fn scheduler_helpers_collect_ready_ops_and_auto_remove_terminal_entries() {
        let op_a = Arc::new(Mutex::new(op_state(OpStatus::Pending, &["alpha"])));
        let mut op_b_state = op_state(OpStatus::Pending, &["beta"]);
        op_b_state.id = "op-2".to_string();
        let op_b = Arc::new(Mutex::new(op_b_state));
        let mut op_c_state = op_state(OpStatus::Completed, &["gamma"]);
        op_c_state.id = "op-3".to_string();
        let op_c = Arc::new(Mutex::new(op_c_state));

        let inner = Arc::new(Mutex::new(Inner {
            ops: HashMap::from([
                ("op-1".to_string(), op_a.clone()),
                ("op-2".to_string(), op_b.clone()),
                ("op-3".to_string(), op_c.clone()),
            ]),
            signals: HashMap::from([
                ("op-1".to_string(), Arc::new(Condvar::new())),
                ("op-2".to_string(), Arc::new(Condvar::new())),
                ("op-3".to_string(), Arc::new(Condvar::new())),
            ]),
            order: VecDeque::from(["op-1".to_string(), "op-2".to_string(), "op-3".to_string()]),
            busy_volumes: HashSet::from(["alpha".to_string()]),
            workers: Vec::new(),
        }));

        let ready = collect_startable(&inner);
        assert_eq!(ready, vec!["op-2".to_string()]);

        let removed_ids = Arc::new(Mutex::new(Vec::<String>::new()));
        let removed_ids_for_emitter = removed_ids.clone();
        schedule_auto_remove(
            "op-3",
            &inner,
            Some(Arc::new(move |id| {
                removed_ids_for_emitter
                    .lock()
                    .expect("removed ids lock")
                    .push(id);
            })),
            Duration::from_millis(10),
        );
        std::thread::sleep(Duration::from_millis(30));

        let guard = inner.lock().expect("ops lock");
        assert!(!guard.ops.contains_key("op-3"));
        assert!(!guard.signals.contains_key("op-3"));
        assert!(!guard.order.contains(&"op-3".to_string()));
        drop(guard);
        assert_eq!(
            removed_ids.lock().expect("removed ids lock").as_slice(),
            ["op-3"]
        );
    }

    #[test]
    fn progress_helpers_update_state_and_cancelled_copies_short_circuit() {
        let (op_arc, resolver, progress_log, ctx) = make_ctx(op_state(OpStatus::Active, &["/"]));

        add_discovered_total(&ctx, 12, None);
        begin_file_progress(&ctx, Path::new("/tmp/example.txt"), 12, None);
        report_chunk_progress(&ctx, 5, None);
        count_skipped_bytes(&ctx, 2);
        finish_file_progress(&ctx, None);
        let tracked_item = { ctx.op_arc.lock().expect("op lock").items[0].clone() };
        advance_item(&ctx, &tracked_item);

        {
            let mut state = op_arc.lock().expect("op lock");
            refresh_rate_locked_with_now(
                &mut state,
                Instant::now() - Duration::from_secs(3),
                Duration::ZERO,
                Instant::now(),
            );
            state.pause.store(true, Ordering::Relaxed);
            state.cancel.store(true, Ordering::Relaxed);
        }
        wait_while_paused(&op_arc, &resolver);

        let mut reader = Cursor::new(b"cancelled".to_vec());
        let mut writer = Cursor::new(Vec::<u8>::new());
        copy_reader_with_progress(&mut reader, &mut writer, &ctx, None).expect("cancelled copy");

        let state = op_arc.lock().expect("op lock");
        assert_eq!(state.total_bytes, 12);
        assert_eq!(state.completed_items, 1);
        assert_eq!(state.copied_bytes, 7);
        assert!(state.sample_count > 0);
        drop(state);

        let emitted = progress_log.lock().expect("progress log lock");
        assert!(!emitted.is_empty());
        assert!(writer.into_inner().is_empty());
    }

    #[test]
    fn resolve_or_park_returns_none_when_cancelled_while_waiting() {
        let fixture = tempdir().expect("temp dir");
        let target = fixture.path().join("target.txt");
        std::fs::write(&target, b"existing").expect("target");

        let (op_arc, resolver, progress_log, ctx) = make_ctx(op_state(OpStatus::Active, &["/"]));
        let cancel_arc = op_arc.clone();
        let resolver_for_thread = resolver.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            cancel_arc
                .lock()
                .expect("op lock")
                .cancel
                .store(true, Ordering::Relaxed);
            resolver_for_thread.notify_all();
        });

        let resolution = resolve_or_park(
            &ctx,
            &OpItem {
                source_path: fixture
                    .path()
                    .join("source.txt")
                    .to_string_lossy()
                    .into_owned(),
                name: "target.txt".to_string(),
                size_bytes: 8,
            },
            &target,
            &None,
        );

        assert!(resolution.is_none());
        let state = op_arc.lock().expect("op lock");
        assert_eq!(state.status, OpStatus::Conflict);
        assert!(state.conflict.is_some());
        drop(state);
        assert!(!progress_log.lock().expect("progress log lock").is_empty());
    }

    #[test]
    fn process_item_rejects_moving_a_directory_into_its_own_descendant() {
        let fixture = tempdir().expect("temp dir");
        let source = fixture.path().join("tree");
        let nested_dest = source.join("nested");
        std::fs::create_dir_all(&nested_dest).expect("nested");
        std::fs::write(source.join("top.txt"), b"top").expect("top");

        let mut state = op_state(OpStatus::Active, &["/"]);
        state.kind = OpKind::Move;
        state.destination_dir = nested_dest.clone();
        state.items = vec![item(&source, 3)];
        let (op_arc, _resolver, _progress_log, ctx) = make_ctx(state);

        process_item(&ctx, &item(&source, 3), &None);

        let state = op_arc.lock().expect("op lock");
        assert!(state
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("Cannot move")));
        drop(state);
        assert!(!nested_dest.join("tree").exists());
    }

    #[test]
    fn process_item_covers_delete_compress_and_extract_paths() {
        let fixture = tempdir().expect("temp dir");

        let delete_source = fixture.path().join("delete.txt");
        std::fs::write(&delete_source, b"delete").expect("delete source");
        let mut delete_state = op_state(OpStatus::Active, &["/"]);
        delete_state.kind = OpKind::Delete;
        delete_state.items = vec![item(&delete_source, 6)];
        let (delete_arc, _resolver, _progress_log, delete_ctx) = make_ctx(delete_state);
        process_item(&delete_ctx, &item(&delete_source, 6), &None);
        assert!(!delete_source.exists());
        assert_eq!(delete_arc.lock().expect("op lock").completed_items, 1);

        let source_dir = fixture.path().join("payload");
        std::fs::create_dir_all(&source_dir).expect("source dir");
        std::fs::write(source_dir.join("inside.txt"), b"payload").expect("source file");
        let archive_path = fixture.path().join("archive.zip");
        let mut compress_state = op_state(OpStatus::Active, &["/"]);
        compress_state.kind = OpKind::Compress;
        compress_state.destination_dir = archive_path.clone();
        compress_state.items = vec![item(&source_dir, 0)];
        let (compress_arc, _resolver, _progress_log, compress_ctx) = make_ctx(compress_state);
        process_item(&compress_ctx, &item(&source_dir, 0), &None);
        assert!(archive_path.exists());
        assert_eq!(compress_arc.lock().expect("op lock").completed_items, 1);

        let extract_root = fixture.path().join("extract");
        let mut extract_state = op_state(OpStatus::Active, &["/"]);
        extract_state.kind = OpKind::Extract;
        extract_state.destination_dir = extract_root.clone();
        extract_state.items = vec![item(&archive_path, 0)];
        let (extract_arc, _resolver, _progress_log, extract_ctx) = make_ctx(extract_state);
        process_item(&extract_ctx, &item(&archive_path, 0), &None);
        assert_eq!(
            std::fs::read(
                extract_root
                    .join("archive")
                    .join("payload")
                    .join("inside.txt")
            )
            .expect("extract"),
            b"payload"
        );
        assert_eq!(extract_arc.lock().expect("op lock").completed_items, 1);
    }

    #[test]
    fn run_operation_marks_success_failure_and_cancelled_terminal_states() {
        let fixture = tempdir().expect("temp dir");

        let delete_source = fixture.path().join("done.txt");
        std::fs::write(&delete_source, b"done").expect("done source");
        let mut completed_state = op_state(OpStatus::Pending, &["alpha"]);
        completed_state.kind = OpKind::Delete;
        completed_state.items = vec![item(&delete_source, 4)];
        completed_state.total_bytes = 4;
        let completed_op = Arc::new(Mutex::new(completed_state));

        let missing_source = fixture.path().join("missing.txt");
        let mut failed_state = op_state(OpStatus::Pending, &["beta"]);
        failed_state.items = vec![item(&missing_source, 5)];
        let failed_op = Arc::new(Mutex::new(failed_state));

        let cancelled_source = fixture.path().join("cancelled.txt");
        std::fs::write(&cancelled_source, b"cancelled").expect("cancelled source");
        let mut cancelled_state = op_state(OpStatus::Pending, &["gamma"]);
        cancelled_state.kind = OpKind::Delete;
        cancelled_state.items = vec![item(&cancelled_source, 9)];
        cancelled_state.cancel.store(true, Ordering::Relaxed);
        let cancelled_op = Arc::new(Mutex::new(cancelled_state));

        let inner = Arc::new(Mutex::new(Inner {
            ops: HashMap::from([
                ("op-1".to_string(), completed_op.clone()),
                ("op-2".to_string(), failed_op.clone()),
                ("op-3".to_string(), cancelled_op.clone()),
            ]),
            signals: HashMap::from([
                ("op-1".to_string(), Arc::new(Condvar::new())),
                ("op-2".to_string(), Arc::new(Condvar::new())),
                ("op-3".to_string(), Arc::new(Condvar::new())),
            ]),
            order: VecDeque::from(["op-1".to_string(), "op-2".to_string(), "op-3".to_string()]),
            busy_volumes: HashSet::new(),
            workers: Vec::new(),
        }));
        let runtime = WorkerRuntime {
            progress: None,
            conflict: None,
            removed: None,
            retention: Duration::from_millis(5),
            rate_window: Duration::ZERO,
            instant_now: Arc::new(Instant::now),
            coordinator: Arc::new(resource_coordinator::ResourceCoordinator::new()),
        };

        run_operation("op-1".to_string(), &inner, &runtime);
        run_operation("op-2".to_string(), &inner, &runtime);
        run_operation("op-3".to_string(), &inner, &runtime);

        let completed = completed_op.lock().expect("completed lock");
        assert_eq!(completed.status, OpStatus::Completed);
        drop(completed);

        let failed = failed_op.lock().expect("failed lock");
        assert_eq!(failed.status, OpStatus::Failed);
        assert!(failed.error_message.is_some());
        drop(failed);

        let cancelled = cancelled_op.lock().expect("cancelled lock");
        assert_eq!(cancelled.status, OpStatus::Cancelled);
    }

    #[test]
    fn release_and_reschedule_dispatches_pending_work_and_removes_terminal_ops() {
        let fixture = tempdir().expect("temp dir");
        let completed_file = fixture.path().join("completed.txt");
        let pending_file = fixture.path().join("pending.txt");
        std::fs::write(&completed_file, b"done").expect("completed");
        std::fs::write(&pending_file, b"wait").expect("pending");

        let mut completed_state = op_state(OpStatus::Completed, &["alpha"]);
        completed_state.kind = OpKind::Delete;
        completed_state.items = vec![item(&completed_file, 4)];
        let mut pending_state = op_state(OpStatus::Pending, &["beta"]);
        pending_state.kind = OpKind::Delete;
        pending_state.items = vec![item(&pending_file, 4)];
        pending_state.id = "op-2".to_string();

        let completed_op = Arc::new(Mutex::new(completed_state));
        let pending_op = Arc::new(Mutex::new(pending_state));
        let removed = Arc::new(Mutex::new(Vec::<String>::new()));
        let removed_for_emitter = removed.clone();

        let inner = Arc::new(Mutex::new(Inner {
            ops: HashMap::from([
                ("op-1".to_string(), completed_op),
                ("op-2".to_string(), pending_op.clone()),
            ]),
            signals: HashMap::from([
                ("op-1".to_string(), Arc::new(Condvar::new())),
                ("op-2".to_string(), Arc::new(Condvar::new())),
            ]),
            order: VecDeque::from(["op-1".to_string(), "op-2".to_string()]),
            busy_volumes: HashSet::from(["alpha".to_string()]),
            workers: Vec::new(),
        }));
        let runtime = WorkerRuntime {
            progress: None,
            conflict: None,
            removed: Some(Arc::new(move |id| {
                removed_for_emitter.lock().expect("removed lock").push(id);
            })),
            retention: Duration::from_millis(10),
            rate_window: Duration::ZERO,
            instant_now: Arc::new(Instant::now),
            coordinator: Arc::new(resource_coordinator::ResourceCoordinator::new()),
        };

        release_and_reschedule("op-1", &inner, &runtime);
        std::thread::sleep(Duration::from_millis(40));

        assert!(!pending_file.exists());
        assert_eq!(
            pending_op.lock().expect("pending lock").status,
            OpStatus::Completed
        );
        assert!(removed
            .lock()
            .expect("removed lock")
            .iter()
            .any(|id| id == "op-1"));
    }

    #[test]
    fn release_and_reschedule_admits_rescheduled_work_through_the_resource_coordinator() {
        let fixture = tempdir().expect("temp dir");
        let completed_file = fixture.path().join("completed.txt");
        let pending_file = fixture.path().join("pending.txt");
        std::fs::write(&completed_file, b"done").expect("completed");
        std::fs::write(&pending_file, b"wait").expect("pending");

        let mut completed_state = op_state(OpStatus::Completed, &["alpha"]);
        completed_state.kind = OpKind::Delete;
        completed_state.items = vec![item(&completed_file, 4)];
        let mut pending_state = op_state(OpStatus::Pending, &["beta"]);
        pending_state.kind = OpKind::Delete;
        pending_state.items = vec![item(&pending_file, 4)];
        pending_state.id = "op-2".to_string();

        let completed_op = Arc::new(Mutex::new(completed_state));
        let pending_op = Arc::new(Mutex::new(pending_state));

        let inner = Arc::new(Mutex::new(Inner {
            ops: HashMap::from([
                ("op-1".to_string(), completed_op),
                ("op-2".to_string(), pending_op.clone()),
            ]),
            signals: HashMap::from([
                ("op-1".to_string(), Arc::new(Condvar::new())),
                ("op-2".to_string(), Arc::new(Condvar::new())),
            ]),
            order: VecDeque::from(["op-1".to_string(), "op-2".to_string()]),
            // "beta" is not legacy-busy: `collect_startable` alone would
            // consider op-2 immediately eligible. The coordinator hold
            // below is the only thing standing between it and admission,
            // which is exactly what this test is proving is now honored.
            busy_volumes: HashSet::from(["alpha".to_string()]),
            workers: Vec::new(),
        }));

        let coordinator = Arc::new(resource_coordinator::ResourceCoordinator::new());
        let runtime = WorkerRuntime {
            progress: None,
            conflict: None,
            removed: None,
            retention: Duration::from_millis(10),
            rate_window: Duration::ZERO,
            instant_now: Arc::new(Instant::now),
            coordinator: Arc::clone(&coordinator),
        };

        // Hold "beta"'s only throughput slot directly through the
        // coordinator before op-2 ever becomes startable, simulating an
        // unrelated in-flight job on that resource.
        let hold = coordinator
            .submit(resource_coordinator::JobSpec::new(
                [resource_coordinator::JobClass::Throughput],
                ["beta".to_string()],
            ))
            .expect("holder granted");

        release_and_reschedule("op-1", &inner, &runtime);

        // Give the rescheduled dispatch thread every chance to run; if it
        // bypassed coordinator admission (the bug this test guards against)
        // op-2 would complete almost immediately despite "beta" being held.
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(
            pending_op.lock().expect("pending lock").status,
            OpStatus::Pending,
            "op-2 must stay pending while its resource's coordinator slot is held, \
             even though it is not in the legacy busy_volumes set"
        );
        assert!(
            pending_file.exists(),
            "op-2 must not have run while admission was denied"
        );

        drop(hold);

        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        while pending_op.lock().expect("pending lock").status == OpStatus::Pending
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            pending_op.lock().expect("pending lock").status,
            OpStatus::Completed,
            "op-2 must run once the coordinator admits it"
        );
        assert!(!pending_file.exists());
    }
}
