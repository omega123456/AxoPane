use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;

use super::compact::new_segment_target;
use super::exclusions;
use super::names::PackedNames;
use super::platform;
use super::segment::{
    walk_spill_path, BuildError, BuiltSegment, GraftTarget, SpillOptions, SpillRecord, WalkSpill,
    RUN_BYTES,
};
use super::tree::{DirectoryListing, FolderSizeTree};
use crate::resource_coordinator::parallelism::walker_job_threads;
use crate::resource_coordinator::{JobClass, JobHandle, JobSpec};
use crate::size::fast_walk::{self, DirRecord, Qos, WalkOptions};
use crate::size::manual::ManualSizeError;

use super::machine::Message;
use super::scan_batch::BatchLane;
use super::service::{Shared, SizeEmitter, WalkTuning, READMIT_EVERY_DIRS, RECORD_BATCH};
use super::split::split_root;

const ITEMS_PER_PROGRESS_REPORT: u64 = 10_000;

#[derive(Default)]
pub(super) struct Lanes {
    pub(super) background: VecDeque<ScanJob>,
    pub(super) boosts: VecDeque<BoostJob>,
    pub(super) running_background: Option<RunningWalk>,
    pub(super) running_boost: Option<(String, Arc<AtomicBool>)>,
    pub(super) power_paused: bool,
    pub(super) shutdown: bool,
}

pub(super) struct RunningWalk {
    pub(super) root: PathBuf,
    pub(super) cancel: Arc<AtomicBool>,
}

impl Lanes {
    pub(super) fn cancel_running_within(&self, excluded: &Path) {
        let background = self
            .running_background
            .as_ref()
            .filter(|running| exclusions::is_within(&running.root, excluded))
            .map(|running| &running.cancel);
        let boost = self
            .running_boost
            .as_ref()
            .filter(|(path, _)| {
                exclusions::is_within(&platform::normalize(Path::new(path)), excluded)
            })
            .map(|(_, cancel)| cancel);
        for cancel in background.into_iter().chain(boost) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn background_must_wait(&self) -> bool {
        self.power_paused || !self.boosts.is_empty() || self.running_boost.is_some()
    }

    fn background_parked(&self) -> bool {
        self.background_must_wait() && !self.shutdown
    }

    pub(super) fn claim_ready_background(&mut self) -> Option<ClaimedJob<ScanJob>> {
        if self.shutdown {
            return None;
        }
        self.claim_background()
    }

    pub(super) fn next_background_is(&self, wanted: impl Fn(&ScanJob) -> bool) -> bool {
        self.background.front().is_some_and(wanted)
    }

    fn claim_background(&mut self) -> Option<ClaimedJob<ScanJob>> {
        if self.background_must_wait() {
            return None;
        }
        let job = self.background.pop_front()?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.running_background = Some(RunningWalk {
            root: job.root.clone(),
            cancel: Arc::clone(&cancel),
        });
        Some(ClaimedJob { job, cancel })
    }

    fn claim_boost(&mut self) -> Option<ClaimedJob<BoostJob>> {
        let job = self.boosts.pop_front()?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.running_boost = Some((job.path.clone(), Arc::clone(&cancel)));
        Some(ClaimedJob { job, cancel })
    }
}

pub(super) struct ClaimedJob<Job> {
    pub(super) job: Job,
    pub(super) cancel: Arc<AtomicBool>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScanKind {
    Update,
    FullSubtree,
    FullRoot,
}

impl ScanKind {
    pub(super) fn is_full(self) -> bool {
        self != Self::Update
    }
}

pub(super) struct ScanJob {
    pub(super) root: PathBuf,
    pub(super) kind: ScanKind,
}

pub(super) struct BoostJob {
    pub(super) path: String,
    pub(super) refresh: bool,
    pub(super) waiters: Vec<SizeEmitter>,
}

pub(super) struct BuiltGraft {
    pub(super) sequence: u64,
    pub(super) built: BuiltSegment,
    pub(super) kept: Vec<PathBuf>,
}

pub(super) enum WalkOutcome {
    Walked,
    Built(BuiltGraft),
    Split,
    Skipped,
    Failed(ManualSizeError),
    BuildFailed,
}

impl WalkOutcome {
    pub(super) fn of_walk(walked: Result<(), ManualSizeError>) -> Self {
        match walked {
            Ok(()) => Self::Walked,
            Err(error) => Self::Failed(error),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct LaneContext<'lane> {
    pub(super) shared: &'lane Shared,
    pub(super) sender: &'lane Sender<Message>,
}

pub(super) struct LaneWalk<'walk> {
    pub(super) root: &'walk Path,
    pub(super) lane: Lane,
    pub(super) cancel: &'walk Arc<AtomicBool>,
}

pub(super) fn run_background(shared: &Shared, sender: &Sender<Message>) {
    let context = LaneContext { shared, sender };
    let mut batches = BatchLane::new(context);
    while let Some(claimed) = next_background_job(shared, &mut batches) {
        if batches.takes(&claimed.job) {
            batches.walk(claimed);
            shared.lanes().running_background = None;
            continue;
        }
        batches.flush();
        let ClaimedJob { job, cancel } = claimed;
        let outcome = background_scan(context, &job, &cancel);
        shared.lanes().running_background = None;
        let _ = sender.send(Message::ScanDone { job, outcome });
    }
}

fn next_background_job(
    shared: &Shared,
    batches: &mut BatchLane<'_>,
) -> Option<ClaimedJob<ScanJob>> {
    if batches.is_open() {
        let ready = shared.lanes().claim_ready_background();
        if ready.is_some() {
            return ready;
        }
        batches.flush();
    }
    wait_for_job(shared, Lanes::claim_background)
}

pub(super) fn run_boosts(shared: &Shared, sender: &Sender<Message>) {
    let context = LaneContext { shared, sender };
    while let Some(ClaimedJob { job, cancel }) = wait_for_job(shared, Lanes::claim_boost) {
        let outcome = admitted_boost_walk(context, &job, &cancel);
        shared.lanes().running_boost = None;
        shared.wake.notify_all();
        let _ = sender.send(Message::BoostDone { job, outcome });
    }
}

fn background_scan(
    context: LaneContext<'_>,
    job: &ScanJob,
    cancel: &Arc<AtomicBool>,
) -> WalkOutcome {
    let lane_walk = LaneWalk {
        root: &job.root,
        lane: Lane::Background {
            full: job.kind.is_full(),
        },
        cancel,
    };
    if job.kind == ScanKind::FullRoot {
        return split_root(context, lane_walk);
    }
    walk(context, lane_walk)
}

fn admitted_boost_walk(
    context: LaneContext<'_>,
    job: &BoostJob,
    cancel: &Arc<AtomicBool>,
) -> WalkOutcome {
    let admission = context.shared.coordinator.submit(JobSpec::new(
        [JobClass::Throughput],
        ["size-index-boost".to_string()],
    ));
    let Ok(_admission) = admission else {
        return WalkOutcome::Failed(ManualSizeError::Cancelled);
    };
    let root = platform::normalize(Path::new(&job.path));
    let lane_walk = LaneWalk {
        root: &root,
        lane: Lane::Boost {
            refresh: job.refresh,
        },
        cancel,
    };
    walk(context, lane_walk)
}

#[derive(Clone, Copy)]
pub(super) enum Lane {
    Background { full: bool },
    Boost { refresh: bool },
}

#[derive(Default)]
struct GraftPlan {
    root_name: String,
    kept: Vec<PathBuf>,
}

pub(super) struct WalkPlan {
    pub(super) options: WalkOptions,
    full: bool,
    skip_indexed: bool,
    graft: GraftPlan,
}

impl Lane {
    pub(super) fn plan(self, shared: &Shared, cancel: &Arc<AtomicBool>) -> WalkPlan {
        match self {
            Lane::Background { full } => WalkPlan {
                options: shared.walk_options(
                    Arc::clone(cancel),
                    WalkTuning {
                        qos: Qos::Background,
                        threads: 1,
                    },
                ),
                full,
                skip_indexed: false,
                graft: GraftPlan::default(),
            },
            Lane::Boost { refresh } => WalkPlan {
                options: shared.walk_options(
                    Arc::clone(cancel),
                    WalkTuning {
                        qos: Qos::UserInitiated,
                        threads: walker_job_threads(),
                    },
                ),
                full: false,
                skip_indexed: !refresh,
                graft: GraftPlan::default(),
            },
        }
    }
}

fn root_name_in(tree: &FolderSizeTree, root: &Path) -> String {
    if let Some(indexed_name) = tree.folder_name(root) {
        return indexed_name.to_string();
    }
    let own_name = root.file_name().unwrap_or(root.as_os_str());
    own_name.to_string_lossy().into_owned()
}

pub(super) fn narrow_walk(shared: &Shared, root: &Path, plan: &mut WalkPlan) -> bool {
    if shared.user_excludes(root) {
        return false;
    }
    let view = shared.view();
    if shared.privacy_blocked(&view, root) {
        return false;
    }
    if !view.full_disk_access {
        plan.options
            .excluded
            .extend(shared.unvisited_protected(&view));
    }
    plan.graft.root_name = root_name_in(&view.tree, root);
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
    plan.graft.kept = view.tree.indexed_subtrees_under(root);
    plan.options
        .excluded
        .extend(plan.graft.kept.iter().cloned());
    true
}

pub(super) struct ClaimedSpill {
    pub(super) sequence: u64,
    pub(super) spill: WalkSpill,
}

impl ClaimedSpill {
    pub(super) fn claim(shared: &Shared, root: &Path) -> Self {
        let sequence = shared.sequences().claim();
        Self {
            sequence,
            spill: WalkSpill::create(SpillOptions {
                runs_path: walk_spill_path(&shared.segment_directory, sequence),
                root: root.to_path_buf(),
                run_bytes: RUN_BYTES,
            }),
        }
    }

    pub(super) fn append(&mut self, record: &DirRecord) -> Result<(), BuildError> {
        let no_file_names = PackedNames::default();
        let file_names = record.file_names.as_ref().unwrap_or(&no_file_names);
        self.spill.append(SpillRecord {
            path: &record.path,
            listing: DirectoryListing {
                direct_bytes: record.direct_bytes,
                subdirs: &record.subdirs,
                denied: record.denied,
            },
            file_names: file_names.as_bytes(),
        })
    }
}

enum GraftFailure {
    Walk(ManualSizeError),
    Spill(BuildError),
    Build(BuildError),
}

pub(super) struct ProgressReport<'report> {
    context: LaneContext<'report>,
    full: bool,
    unreported_items: u64,
}

impl<'report> ProgressReport<'report> {
    pub(super) fn new(context: LaneContext<'report>, full: bool) -> Self {
        Self {
            context,
            full,
            unreported_items: 0,
        }
    }

    pub(super) fn add(&mut self, items: u64) {
        self.unreported_items += items;
        if self.unreported_items >= ITEMS_PER_PROGRESS_REPORT {
            self.send();
        }
    }

    pub(super) fn send(&mut self) {
        let items = std::mem::take(&mut self.unreported_items);
        if self.full && items > 0 {
            let _ = self.context.sender.send(Message::Progress(items));
        }
    }
}

struct RecordSink<'walk> {
    context: LaneContext<'walk>,
    lane_walk: &'walk LaneWalk<'walk>,
    full: bool,
    buffered: Vec<DirRecord>,
    spill: Option<ClaimedSpill>,
    spill_failure: Option<BuildError>,
    progress: ProgressReport<'walk>,
}

impl<'walk> RecordSink<'walk> {
    fn new(context: LaneContext<'walk>, lane_walk: &'walk LaneWalk<'walk>, full: bool) -> Self {
        Self {
            context,
            lane_walk,
            full,
            buffered: Vec::new(),
            spill: None,
            spill_failure: None,
            progress: ProgressReport::new(context, full),
        }
    }

    fn accept(&mut self, record: DirRecord) {
        if self.spill_failure.is_some() {
            return;
        }
        let heap_batch_records = self.context.shared.batching.heap_records;
        let fits_in_heap_batch = self.spill.is_none() && self.buffered.len() < heap_batch_records;
        if fits_in_heap_batch {
            self.buffered.push(record);
            return;
        }
        if let Err(error) = self.spill_record(record) {
            self.spill_failure = Some(error);
            self.lane_walk.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn spill_record(&mut self, record: DirRecord) -> Result<(), BuildError> {
        let LaneContext { shared, .. } = self.context;
        let root = self.lane_walk.root;
        let claimed = self
            .spill
            .get_or_insert_with(|| ClaimedSpill::claim(shared, root));
        let mut spilled_items = 0;
        for spilled in self.buffered.drain(..).chain([record]) {
            claimed.append(&spilled)?;
            spilled_items += 1 + spilled.file_count;
        }
        self.progress.add(spilled_items);
        Ok(())
    }

    fn finish(mut self, walked: Result<(), ManualSizeError>, graft: GraftPlan) -> WalkOutcome {
        self.progress.send();
        let Some(claimed) = self.spill.take() else {
            return self.finish_in_the_heap(walked);
        };
        let sequence = claimed.sequence;
        let GraftPlan { root_name, kept } = graft;
        let built = self
            .spilled_walk(walked)
            .and_then(|()| self.build_graft(claimed, root_name));
        match built {
            Ok(built) => WalkOutcome::Built(BuiltGraft {
                sequence,
                built,
                kept,
            }),
            Err(failure) => self.abandon_graft(sequence, failure),
        }
    }

    fn finish_in_the_heap(self, walked: Result<(), ManualSizeError>) -> WalkOutcome {
        send_records(self.context, self.buffered, self.full);
        WalkOutcome::of_walk(walked)
    }

    fn spilled_walk(&mut self, walked: Result<(), ManualSizeError>) -> Result<(), GraftFailure> {
        if let Some(error) = self.spill_failure.take() {
            return Err(GraftFailure::Spill(error));
        }
        walked.map_err(GraftFailure::Walk)
    }

    fn build_graft(
        &self,
        claimed: ClaimedSpill,
        root_name: String,
    ) -> Result<BuiltSegment, GraftFailure> {
        let directory = &self.context.shared.segment_directory;
        let target = GraftTarget {
            segment: new_segment_target(directory, claimed.sequence),
            root_name,
        };
        let cancel = self.lane_walk.cancel;
        let is_cancelled = || cancel.load(Ordering::Relaxed);
        claimed
            .spill
            .build_unless_stopped(target, &is_cancelled)
            .map_err(GraftFailure::Build)
    }

    fn abandon_graft(&self, sequence: u64, failure: GraftFailure) -> WalkOutcome {
        self.context.shared.sequences().release(sequence);
        let was_cancelled = self.lane_walk.cancel.load(Ordering::Relaxed);
        match failure {
            GraftFailure::Walk(error) => WalkOutcome::Failed(error),
            GraftFailure::Build(_) if was_cancelled => {
                WalkOutcome::Failed(ManualSizeError::Cancelled)
            }
            GraftFailure::Spill(error) | GraftFailure::Build(error) => {
                log::warn!(
                    "size index: the walk of {} could not be saved: {error:?}",
                    self.lane_walk.root.display()
                );
                WalkOutcome::BuildFailed
            }
        }
    }
}

pub(super) fn send_records(context: LaneContext<'_>, records: Vec<DirRecord>, full: bool) {
    let mut unsent = records.into_iter().peekable();
    while unsent.peek().is_some() {
        let records = unsent.by_ref().take(RECORD_BATCH).collect();
        let _ = context.sender.send(Message::Records { records, full });
    }
}

pub(super) fn walk(context: LaneContext<'_>, lane_walk: LaneWalk<'_>) -> WalkOutcome {
    let LaneContext { shared, sender } = context;
    let LaneWalk { root, lane, cancel } = lane_walk;
    let mut plan = lane.plan(shared, cancel);
    if !narrow_walk(shared, root, &mut plan) {
        return WalkOutcome::Skipped;
    }
    let _ = sender.send(Message::WalkStarted {
        root: root.to_path_buf(),
        lane,
    });
    let sink = Mutex::new(RecordSink::new(context, &lane_walk, plan.full));
    let gentle =
        matches!(lane, Lane::Background { .. }).then(|| Mutex::new(Gentle::admit(shared, cancel)));
    let walked = fast_walk::subtree_records(root, &plan.options, |record| {
        sink.lock().expect("record sink lock").accept(record);
        if let Some(gentle) = &gentle {
            gentle
                .lock()
                .expect("gentle walk lock")
                .after_record(BackgroundTurn { shared, cancel });
        }
    });
    let sink = sink.into_inner().expect("record sink lock");
    sink.finish(walked, plan.graft)
}

#[derive(Clone, Copy)]
pub(super) struct BackgroundTurn<'turn> {
    pub(super) shared: &'turn Shared,
    pub(super) cancel: &'turn AtomicBool,
}

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

    pub(super) fn after_record(&mut self, turn: BackgroundTurn<'_>) {
        let BackgroundTurn { shared, cancel } = turn;
        self.since_admitted += 1;
        let must_wait = shared.lanes().background_must_wait();
        let turn_due = self.since_admitted >= READMIT_EVERY_DIRS || must_wait;
        if !turn_due {
            return;
        }
        self.admission = None;
        wait_for_background_turn(shared);
        *self = Self::admit(shared, cancel);
    }
}

fn wait_for_background_turn(shared: &Shared) {
    let mut lanes = shared.lanes();
    while lanes.background_parked() {
        #[cfg(feature = "test-utils")]
        (shared.on_background_parked)();
        lanes = shared.wake.wait(lanes).expect("index lanes lock");
    }
}
