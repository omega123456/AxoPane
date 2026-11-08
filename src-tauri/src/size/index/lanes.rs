use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;

use super::platform;
use crate::resource_coordinator::{JobClass, JobHandle, JobSpec};
use crate::size::fast_walk::{self, DirRecord, Qos, WalkOptions, MAX_JOB_THREADS};
use crate::size::manual::ManualSizeError;

use super::machine::Message;
use super::service::{Shared, SizeEmitter, READMIT_EVERY_DIRS, RECORD_BATCH};

#[derive(Default)]
pub(super) struct Lanes {
    pub(super) background: VecDeque<ScanJob>,
    pub(super) boosts: VecDeque<BoostJob>,
    pub(super) running_background: Option<Arc<AtomicBool>>,
    pub(super) running_boost: Option<(String, Arc<AtomicBool>)>,
    /// Sleep, Low Power Mode or thermal pressure.
    pub(super) power_paused: bool,
    pub(super) shutdown: bool,
}

impl Lanes {
    pub(super) fn background_must_wait(&self) -> bool {
        self.power_paused || !self.boosts.is_empty() || self.running_boost.is_some()
    }

    fn claim_background(&mut self) -> Option<ClaimedJob<ScanJob>> {
        if self.background_must_wait() {
            return None;
        }
        let job = self.background.pop_front()?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.running_background = Some(Arc::clone(&cancel));
        Some(ClaimedJob { job, cancel })
    }

    fn claim_boost(&mut self) -> Option<ClaimedJob<BoostJob>> {
        let job = self.boosts.pop_front()?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.running_boost = Some((job.path.clone(), Arc::clone(&cancel)));
        Some(ClaimedJob { job, cancel })
    }
}

struct ClaimedJob<Job> {
    job: Job,
    cancel: Arc<AtomicBool>,
}

/// Blocks until `claim` hands out a job; `None` once the index shuts down.
fn wait_for_job<Job>(
    shared: &Shared,
    claim: impl Fn(&mut Lanes) -> Option<ClaimedJob<Job>>,
) -> Option<ClaimedJob<Job>> {
    let mut lanes = shared.lanes();
    loop {
        if lanes.shutdown {
            return None;
        }
        if let Some(claimed) = claim(&mut lanes) {
            return Some(claimed);
        }
        lanes = shared.wake.wait(lanes).expect("index lanes lock");
    }
}

pub(super) struct ScanJob {
    pub(super) root: PathBuf,
    /// Part of a cold or full rescan (counts toward progress).
    pub(super) full: bool,
}

pub(super) struct BoostJob {
    pub(super) path: String,
    pub(super) refresh: bool,
    pub(super) waiters: Vec<SizeEmitter>,
}

pub(super) enum WalkOutcome {
    Walked,
    /// Nothing to walk: privacy-blocked or already indexed.
    Skipped,
    Failed(ManualSizeError),
}

pub(super) fn run_background(shared: &Shared, sender: &Sender<Message>) {
    while let Some(ClaimedJob { job, cancel }) = wait_for_job(shared, Lanes::claim_background) {
        let outcome = walk(
            shared,
            sender,
            &job.root,
            Lane::Background { full: job.full },
            &cancel,
        );
        shared.lanes().running_background = None;
        let _ = sender.send(Message::ScanDone { job, outcome });
    }
}

pub(super) fn run_boosts(shared: &Shared, sender: &Sender<Message>) {
    while let Some(ClaimedJob { job, cancel }) = wait_for_job(shared, Lanes::claim_boost) {
        let outcome = admitted_boost_walk(shared, sender, &job, &cancel);
        shared.lanes().running_boost = None;
        shared.wake.notify_all();
        let _ = sender.send(Message::BoostDone { job, outcome });
    }
}

fn admitted_boost_walk(
    shared: &Shared,
    sender: &Sender<Message>,
    job: &BoostJob,
    cancel: &Arc<AtomicBool>,
) -> WalkOutcome {
    let admission = shared.coordinator.submit(JobSpec::new(
        [JobClass::Throughput],
        ["size-index-boost".to_string()],
    ));
    let Ok(_admission) = admission else {
        return WalkOutcome::Failed(ManualSizeError::Cancelled);
    };
    walk(
        shared,
        sender,
        &platform::normalize(Path::new(&job.path)),
        Lane::Boost {
            refresh: job.refresh,
        },
        cancel,
    )
}

#[derive(Clone, Copy)]
pub(super) enum Lane {
    /// Gentle: one thread, background QoS. `full` counts as indexing.
    Background { full: bool },
    /// Four threads, user-initiated QoS. Unless refreshing, subtrees the
    /// index already covers are skipped.
    Boost { refresh: bool },
}

struct WalkPlan {
    options: WalkOptions,
    /// Part of a cold or full rescan (counts toward progress).
    full: bool,
    skip_indexed: bool,
}

impl Lane {
    fn plan(self, shared: &Shared, cancel: &Arc<AtomicBool>) -> WalkPlan {
        match self {
            Lane::Background { full } => WalkPlan {
                options: shared.walk_options(Arc::clone(cancel), Qos::Background, 1),
                full,
                skip_indexed: false,
            },
            Lane::Boost { refresh } => WalkPlan {
                options: shared.walk_options(
                    Arc::clone(cancel),
                    Qos::UserInitiated,
                    MAX_JOB_THREADS,
                ),
                full: false,
                skip_indexed: !refresh,
            },
        }
    }
}

/// Excludes what the walk must not read or need not re-read; `false` when
/// nothing under `root` is left to walk.
fn narrow_walk(shared: &Shared, root: &Path, plan: &mut WalkPlan) -> bool {
    let view = shared.view();
    if shared.privacy_blocked(&view, root) {
        return false;
    }
    if !view.full_disk_access {
        plan.options
            .excluded
            .extend(shared.unvisited_protected(&view));
    }
    if !plan.skip_indexed {
        return true;
    }
    let already_indexed = view
        .tree
        .total(root)
        .is_some_and(|total| total.fully_indexed && !total.skipped);
    if already_indexed {
        return false;
    }
    plan.options
        .excluded
        .extend(view.tree.indexed_subtrees_under(root));
    true
}

/// Walks `root`, streaming record batches to the coordination thread.
pub(super) fn walk(
    shared: &Shared,
    sender: &Sender<Message>,
    root: &Path,
    lane: Lane,
    cancel: &Arc<AtomicBool>,
) -> WalkOutcome {
    let mut plan = lane.plan(shared, cancel);
    if !narrow_walk(shared, root, &mut plan) {
        return WalkOutcome::Skipped;
    }

    let full = plan.full;
    let batch = Mutex::new(Vec::with_capacity(RECORD_BATCH));
    let flush = |batch: &mut Vec<DirRecord>| {
        if !batch.is_empty() {
            let records = std::mem::take(batch);
            let _ = sender.send(Message::Records { records, full });
        }
    };
    let gentle =
        matches!(lane, Lane::Background { .. }).then(|| Mutex::new(Gentle::admit(shared, cancel)));
    let result = fast_walk::subtree_records(root, &plan.options, |record| {
        let mut pending = batch.lock().expect("record batch lock");
        pending.push(record);
        if pending.len() >= RECORD_BATCH {
            flush(&mut pending);
        }
        if let Some(gentle) = &gentle {
            gentle
                .lock()
                .expect("gentle walk lock")
                .after_record(shared, cancel, || flush(&mut pending));
        }
    });
    flush(&mut batch.lock().expect("record batch lock"));
    match result {
        Ok(()) => WalkOutcome::Walked,
        Err(error) => WalkOutcome::Failed(error),
    }
}

/// Background-walk manners: hold Throughput admission for at most
/// [`READMIT_EVERY_DIRS`] directories, and step aside (releasing it)
/// while the power gate is closed or a boost waits.
pub(super) struct Gentle {
    pub(super) admission: Option<JobHandle>,
    pub(super) since_admitted: u64,
}

impl Gentle {
    pub(super) fn admit(shared: &Shared, cancel: &AtomicBool) -> Self {
        let admission = shared
            .coordinator
            .submit(JobSpec::new(
                [JobClass::Throughput],
                ["size-index".to_string()],
            ))
            .ok();
        if admission.is_none() {
            cancel.store(true, Ordering::Relaxed);
        }
        Self {
            admission,
            since_admitted: 0,
        }
    }

    pub(super) fn after_record(
        &mut self,
        shared: &Shared,
        cancel: &AtomicBool,
        flush: impl FnOnce(),
    ) {
        self.since_admitted += 1;
        let must_wait = shared.lanes().background_must_wait();
        if self.since_admitted < READMIT_EVERY_DIRS && !must_wait {
            return;
        }
        flush();
        self.admission = None;
        wait_for_background_turn(shared);
        *self = Self::admit(shared, cancel);
    }
}

fn wait_for_background_turn(shared: &Shared) {
    let mut lanes = shared.lanes();
    while lanes.background_must_wait() && !lanes.shutdown {
        #[cfg(feature = "test-utils")]
        (shared.on_background_parked)();
        lanes = shared.wake.wait(lanes).expect("index lanes lock");
    }
}
