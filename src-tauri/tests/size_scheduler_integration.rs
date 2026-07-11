use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use file_explorer_lib::size::scheduler::{SizeScheduler, MAX_SIZE_METADATA_ENTRIES};

fn token() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[test]
fn duplicate_requests_coalesce_and_cancel_removes_queued_work() {
    let scheduler = SizeScheduler::new(4);
    assert!(scheduler.schedule("/tmp/a".to_string(), token(), 1_u8));
    assert!(!scheduler.schedule("/tmp/a".to_string(), token(), 2_u8));
    assert_eq!(scheduler.pending_len(), 1);
    assert!(scheduler.cancel("/tmp/a"));
    assert_eq!(scheduler.pending_len(), 0);
    scheduler.shutdown();
    assert!(scheduler.next().is_none());
}

#[test]
fn cancel_many_removes_only_the_requested_queued_jobs() {
    let scheduler = SizeScheduler::new(4);
    assert!(scheduler.schedule("/tmp/alpha".to_string(), token(), 1_u8));
    assert!(scheduler.schedule("/tmp/beta".to_string(), token(), 2_u8));
    assert!(scheduler.schedule("/tmp/gamma".to_string(), token(), 3_u8));

    assert_eq!(
        scheduler.cancel_many(&[
            "/tmp/alpha".to_string(),
            "/tmp/missing".to_string(),
            "/tmp/gamma".to_string(),
        ]),
        2
    );
    assert_eq!(scheduler.pending_len(), 1);

    let remaining = scheduler.next().expect("the untouched job remains queued");
    assert_eq!(remaining.path, "/tmp/beta");
    assert_eq!(remaining.payload, 2);
    scheduler.complete(&remaining.path, &remaining.cancelled);
    scheduler.shutdown();
}

#[test]
fn completed_job_is_removed_only_by_its_own_cancellation_token() {
    let scheduler = SizeScheduler::new(2);
    let cancel = token();
    assert!(scheduler.schedule("/tmp/a".to_string(), Arc::clone(&cancel), 1_u8));
    let job = scheduler.next().expect("job");
    // The token returned inside `job.cancelled` is the *exact same* `Arc`
    // the caller passed to `schedule` — this is what makes `complete`'s
    // `Arc::ptr_eq` check a real identity check instead of comparing against
    // a disconnected scheduler-internal token that could never match.
    assert!(Arc::ptr_eq(&job.cancelled, &cancel));
    scheduler.complete(&job.path, &cancel);
    assert_eq!(scheduler.pending_len(), 0);
    const { assert!(MAX_SIZE_METADATA_ENTRIES >= 10_000) };
    scheduler.shutdown();
}

/// A `complete` call presenting a *different* token than the one the entry
/// was scheduled with must not remove it — this is the safety property that
/// makes cancel-then-reschedule-the-same-path safe: a stale completion from
/// an old, since-superseded job can never tear down a newer job that
/// happens to share the same path.
#[test]
fn complete_with_a_mismatched_token_does_not_remove_the_entry() {
    let scheduler = SizeScheduler::new(2);
    let real_cancel = token();
    assert!(scheduler.schedule("/tmp/a".to_string(), Arc::clone(&real_cancel), 1_u8));

    let impostor_cancel = token();
    scheduler.complete("/tmp/a", &impostor_cancel);
    assert_eq!(
        scheduler.pending_len(),
        1,
        "a mismatched token must not be able to remove another job's entry"
    );

    scheduler.complete("/tmp/a", &real_cancel);
    assert_eq!(scheduler.pending_len(), 0);
    scheduler.shutdown();
}

/// The scheduler's payload no longer needs `Clone`: `next`/`claim` move the
/// payload out by taking it, which is what lets a `SizeService` manual job
/// carry a one-shot `Box<dyn FnOnce>` traversal closure through the queue
/// instead of a dedicated thread being spawned per requested path.
#[test]
fn non_clone_payload_travels_through_the_queue_by_move() {
    let scheduler: SizeScheduler<Box<dyn FnOnce() -> u64 + Send>> = SizeScheduler::new(4);
    assert!(scheduler.schedule("/tmp/a".to_string(), token(), Box::new(|| 42)));
    let job = scheduler.next().expect("job");
    assert_eq!((job.payload)(), 42);
    scheduler.complete(&job.path, &job.cancelled);
    scheduler.shutdown();
}

/// A job's entry (and thus its coalescing/capacity slot) stays present after
/// its payload has been claimed but before `complete` is called — this is
/// what keeps a second `schedule` call for the same still-executing path
/// coalescing onto the in-flight job instead of silently being accepted as a
/// second, independent one.
#[test]
fn claimed_but_not_yet_completed_job_still_blocks_a_duplicate_schedule() {
    let scheduler = SizeScheduler::new(4);
    let first_cancel = token();
    assert!(scheduler.schedule("/tmp/a".to_string(), Arc::clone(&first_cancel), 1_u8));
    let job = scheduler.next().expect("job claimed");
    // Still occupies its slot: a second schedule for the same path is
    // rejected (coalescing), not accepted as an independent second job.
    assert!(!scheduler.schedule("/tmp/a".to_string(), token(), 2_u8));
    assert_eq!(scheduler.pending_len(), 1);
    scheduler.complete(&job.path, &first_cancel);
    assert_eq!(scheduler.pending_len(), 0);
    // Now that the job is fully complete, the path is schedulable again.
    assert!(scheduler.schedule("/tmp/a".to_string(), token(), 3_u8));
    scheduler.shutdown();
}

/// A payload can only ever be claimed once: concurrent `next`/`claim` callers
/// racing for the same queued path never both observe `Some`.
#[test]
fn a_queued_payload_is_claimed_by_exactly_one_caller() {
    let scheduler = Arc::new(SizeScheduler::new(4));
    assert!(scheduler.schedule("/tmp/a".to_string(), token(), 7_u8));

    let claims = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let scheduler = Arc::clone(&scheduler);
            let claims = Arc::clone(&claims);
            thread::spawn(move || {
                if scheduler.claim("/tmp/a").is_some() {
                    claims.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("worker thread");
    }

    assert_eq!(claims.load(Ordering::SeqCst), 1);
    scheduler.shutdown();
}
