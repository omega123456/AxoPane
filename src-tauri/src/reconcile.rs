//! Non-blocking focus reconciliation.
//!
//! [`ReconcileCoordinator::request_reconcile`] is the only thing the
//! window-focus callback (`lib.rs`'s `on_window_event`) is allowed to call
//! synchronously. It bumps a monotonic generation counter and submits one
//! `JobClass::Latency` job to the [`crate::resource_coordinator::ResourceCoordinator`],
//! then returns immediately — no filesystem or volume discovery I/O ever
//! runs on the calling (Tauri event) thread.
//!
//! The actual snapshot comparison happens inside the coordinator-admitted
//! background task supplied by the caller (`compare_and_publish`). Because
//! `request_reconcile` records the generation it just requested *before*
//! submitting the job, and the background task checks
//! [`ReconcileCoordinator::is_current`] immediately before publishing, a
//! slower/older generation's result can never overwrite a newer one, and
//! rapid repeated focus events collapse: only the latest generation's
//! comparison is allowed to publish, and an already-running comparison is
//! left to finish (its result is simply discarded if superseded) rather than
//! spawning a second overlapping job for the same reconcile target.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

/// Tracks the monotonic "latest requested" generation for one reconcile
/// target. Each target must drain independently: a volume refresh must never
/// coalesce away the distinct watched-directory refresh requested by the same
/// focus event.
#[derive(Default)]
struct ReconcileTarget {
    generation: AtomicU64,
    scheduled: AtomicBool,
}

/// Tracks independently coalesced focus-reconcile targets. Owned as Tauri
/// state alongside [`ResourceCoordinator`]; `lib.rs` calls
/// [`ReconcileCoordinator::request_reconcile`] from the window-focus
/// callback and passes it a closure that performs the real (potentially
/// slow) snapshot comparison work to run once admitted.
#[derive(Default)]
pub struct ReconcileCoordinator {
    targets: Mutex<HashMap<String, Arc<ReconcileTarget>>>,
}

impl ReconcileCoordinator {
    fn target(&self, resource_key: &str) -> Arc<ReconcileTarget> {
        let mut targets = self.targets.lock().expect("reconcile targets lock");
        Arc::clone(
            targets
                .entry(resource_key.to_string())
                .or_insert_with(|| Arc::new(ReconcileTarget::default())),
        )
    }

    /// Bumps and returns the new "latest requested" generation. Call this
    /// synchronously from the focus callback before spawning any background
    /// work.
    pub fn bump_generation(&self, resource_key: &str) -> u64 {
        self.target(resource_key)
            .generation
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }

    /// True if `generation` is still the most recently requested one (i.e.
    /// no later `bump_generation` call has happened since). A background
    /// comparison task must check this immediately before publishing any
    /// result — a stale generation's result must never commit.
    pub fn is_current(&self, resource_key: &str, generation: u64) -> bool {
        self.target(resource_key).generation.load(Ordering::SeqCst) == generation
    }

    /// Requests a background reconcile: bumps the generation, then submits a
    /// `Latency`-class job to `coordinator` and, if no worker is already
    /// draining requests, spawns one background worker. Returns immediately —
    /// `coordinator.submit` may block admitting the *thread*, not the
    /// caller, since the actual `submit` call and `work` invocation both
    /// happen inside the spawned thread.
    ///
    /// `work` is responsible for checking [`ReconcileCoordinator::is_current`]
    /// itself immediately before publishing any observable result (emitting
    /// an event, mutating shared state) — this function only guarantees the
    /// generation was current *at request time*; a slower comparison must
    /// still re-check before committing.
    pub fn request_reconcile(
        self: &Arc<Self>,
        coordinator: Arc<ResourceCoordinator>,
        resource_key: String,
        work: impl Fn(u64) + Send + Sync + 'static,
    ) -> u64 {
        let target = self.target(&resource_key);
        let generation = target.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if target.scheduled.swap(true, Ordering::SeqCst) {
            return generation;
        }
        std::thread::spawn(move || {
            drain_reconcile_requests(&target, &coordinator, &resource_key, generation, &work);
        });

        generation
    }
}

#[cfg(feature = "test-utils")]
pub fn generation_for_tests(coordinator: &ReconcileCoordinator) -> u64 {
    coordinator.target("test").generation.load(Ordering::SeqCst)
}

/// A request may arrive while a pass is admitted or running. Drains until no
/// newer generation exists, running only the latest one instead of every
/// intermediate focus event.
fn drain_reconcile_requests(
    target: &ReconcileTarget,
    coordinator: &ResourceCoordinator,
    resource_key: &str,
    first_generation: u64,
    work: &impl Fn(u64),
) {
    let mut requested_generation = first_generation;
    loop {
        run_if_current(
            target,
            coordinator,
            resource_key,
            requested_generation,
            work,
        );
        let Some(next_generation) = next_requested_generation(target, requested_generation) else {
            return;
        };
        requested_generation = next_generation;
    }
}

fn run_if_current(
    target: &ReconcileTarget,
    coordinator: &ResourceCoordinator,
    resource_key: &str,
    requested_generation: u64,
    work: &impl Fn(u64),
) {
    let spec = JobSpec::new([JobClass::Latency], [resource_key.to_string()]);
    let Ok(_permit) = coordinator.submit(spec) else {
        return;
    };
    if target.generation.load(Ordering::SeqCst) == requested_generation {
        work(requested_generation);
    }
}

/// `None` once no newer generation was requested. Re-reading the generation
/// after clearing `scheduled` closes the race where a focus event arrives
/// between the equality check and the clear.
fn next_requested_generation(target: &ReconcileTarget, requested_generation: u64) -> Option<u64> {
    let latest = target.generation.load(Ordering::SeqCst);
    if latest != requested_generation {
        return Some(latest);
    }
    target.scheduled.store(false, Ordering::SeqCst);
    let unchanged = target.generation.load(Ordering::SeqCst) == latest;
    if unchanged || target.scheduled.swap(true, Ordering::SeqCst) {
        return None;
    }
    Some(target.generation.load(Ordering::SeqCst))
}
