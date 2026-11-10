use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::size::fast_walk::{self, DirRecord};

use super::compact::new_segment_target;
use super::lanes::{
    narrow_walk, BackgroundTurn, ClaimedJob, ClaimedSpill, Gentle, Lane, LaneContext,
    ProgressReport, ScanJob, ScanKind, WalkOutcome,
};
use super::machine::Message;
use super::segment::{BuildError, BuiltSegment, GraftTarget};
use super::service::Shared;

pub(super) struct BuiltForest {
    pub(super) sequence: u64,
    pub(super) built: BuiltSegment,
    pub(super) root: PathBuf,
    pub(super) tops: Vec<PathBuf>,
}

pub(super) enum ForestOutcome {
    Built(BuiltForest),
    Failed,
}

pub(super) struct BatchedWalk {
    pub(super) job: ScanJob,
    pub(super) outcome: WalkOutcome,
}

pub(super) struct FinishedBatch {
    pub(super) forest: ForestOutcome,
    pub(super) walks: Vec<BatchedWalk>,
}

fn index_root_of<'shared>(shared: &'shared Shared, path: &Path) -> Option<&'shared PathBuf> {
    shared.roots.iter().find(|root| path.starts_with(root))
}

fn root_record(root: &Path) -> DirRecord {
    DirRecord {
        path: root.to_path_buf(),
        direct_bytes: 0,
        file_count: 0,
        subdirs: Vec::new(),
        denied: false,
        file_names: None,
    }
}

struct ScanBatch {
    root: PathBuf,
    spill: ClaimedSpill,
    walks: Vec<BatchedWalk>,
    spill_failure: Option<BuildError>,
}

impl ScanBatch {
    fn open(shared: &Shared, root: &Path) -> Self {
        let mut spill = ClaimedSpill::claim(shared, root);
        let spill_failure = spill.append(&root_record(root)).err();
        Self {
            root: root.to_path_buf(),
            spill,
            walks: Vec::new(),
            spill_failure,
        }
    }

    fn holds(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    fn is_due(&self, forest_records: u64) -> bool {
        let is_large = self.spill.spill.record_count() >= forest_records;
        is_large || self.spill_failure.is_some()
    }

    fn finish(self, shared: &Shared) -> FinishedBatch {
        let ScanBatch {
            root,
            spill,
            walks,
            spill_failure,
        } = self;
        let sequence = spill.sequence;
        let tops: Vec<PathBuf> = walks.iter().map(|walk| walk.job.root.clone()).collect();
        let built = match spill_failure {
            Some(error) => Err(error),
            None => build_forest(shared, BatchSpill { root: &root, spill }),
        };
        let forest = forest_outcome(
            ForestParts {
                sequence,
                root,
                tops,
            },
            built,
        );
        if !matches!(forest, ForestOutcome::Built(_)) {
            shared.sequences().release(sequence);
        }
        FinishedBatch { forest, walks }
    }
}

struct BatchSpill<'batch> {
    root: &'batch Path,
    spill: ClaimedSpill,
}

fn build_forest(shared: &Shared, batch: BatchSpill<'_>) -> Result<BuiltSegment, BuildError> {
    let BatchSpill { root, spill } = batch;
    let target = GraftTarget {
        segment: new_segment_target(&shared.segment_directory, spill.sequence),
        root_name: root.to_string_lossy().into_owned(),
    };
    let is_shutting_down = || shared.lanes().shutdown;
    spill.spill.build_unless_stopped(target, &is_shutting_down)
}

struct ForestParts {
    sequence: u64,
    root: PathBuf,
    tops: Vec<PathBuf>,
}

fn forest_outcome(parts: ForestParts, built: Result<BuiltSegment, BuildError>) -> ForestOutcome {
    let ForestParts {
        sequence,
        root,
        tops,
    } = parts;
    match built {
        Ok(built) => ForestOutcome::Built(BuiltForest {
            sequence,
            built,
            root,
            tops,
        }),
        Err(error) => {
            log::warn!(
                "size index: a full-scan batch under {} could not be saved: {error:?}",
                root.display()
            );
            ForestOutcome::Failed
        }
    }
}

struct BatchSink<'batch> {
    batch: &'batch mut ScanBatch,
    progress: ProgressReport<'batch>,
}

impl BatchSink<'_> {
    fn accept(&mut self, record: &DirRecord, cancel: &AtomicBool) {
        if self.batch.spill_failure.is_some() {
            return;
        }
        if let Err(error) = self.batch.spill.append(record) {
            self.batch.spill_failure = Some(error);
            cancel.store(true, Ordering::Relaxed);
            return;
        }
        self.progress.add(1 + record.file_count);
    }
}

pub(super) struct BatchLane<'lane> {
    context: LaneContext<'lane>,
    open: Option<ScanBatch>,
}

impl<'lane> BatchLane<'lane> {
    pub(super) fn new(context: LaneContext<'lane>) -> Self {
        Self {
            context,
            open: None,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(super) fn takes(&self, job: &ScanJob) -> bool {
        let is_subtree_of_a_full_scan = job.kind == ScanKind::FullSubtree;
        is_subtree_of_a_full_scan && index_root_of(self.context.shared, &job.root).is_some()
    }

    pub(super) fn walk(&mut self, claimed: ClaimedJob<ScanJob>) {
        let ClaimedJob { job, cancel } = claimed;
        let context = self.context;
        let LaneContext { shared, sender } = context;
        let lane = Lane::Background { full: true };
        let mut plan = lane.plan(shared, &cancel);
        if !narrow_walk(shared, &job.root, &mut plan) {
            let _ = sender.send(Message::ScanDone {
                job,
                outcome: WalkOutcome::Skipped,
            });
            return;
        }
        let _ = sender.send(Message::WalkStarted {
            root: job.root.clone(),
            lane,
        });
        let batch = self.batch_for(&job.root);
        let sink = Mutex::new(BatchSink {
            batch,
            progress: ProgressReport::new(context, true),
        });
        let gentle = Mutex::new(Gentle::admit(shared, &cancel));
        let turn = BackgroundTurn {
            shared,
            cancel: &cancel,
        };
        let walked = fast_walk::subtree_records(&job.root, &plan.options, |record| {
            sink.lock()
                .expect("batch sink lock")
                .accept(&record, &cancel);
            gentle.lock().expect("gentle walk lock").after_record(turn);
        });
        let mut sink = sink.into_inner().expect("batch sink lock");
        sink.progress.send();
        let outcome = WalkOutcome::of_walk(walked);
        sink.batch.walks.push(BatchedWalk { job, outcome });
        self.flush_if_due();
    }

    fn batch_for(&mut self, path: &Path) -> &mut ScanBatch {
        let holds_path = self.open.as_ref().is_some_and(|batch| batch.holds(path));
        if !holds_path {
            self.flush();
        }
        let shared = self.context.shared;
        let root = index_root_of(shared, path).map_or(path, PathBuf::as_path);
        self.open
            .get_or_insert_with(|| ScanBatch::open(shared, root))
    }

    fn flush_if_due(&mut self) {
        let Some(batch) = &self.open else {
            return;
        };
        let next_joins_the_batch = self.context.shared.lanes().next_background_is(|next| {
            next.kind == ScanKind::FullSubtree && batch.holds(&next.root)
        });
        let forest_records = self.context.shared.batching.forest_records;
        if batch.is_due(forest_records) || !next_joins_the_batch {
            self.flush();
        }
    }

    pub(super) fn flush(&mut self) {
        let Some(batch) = self.open.take() else {
            return;
        };
        let shared = self.context.shared;
        if shared.lanes().shutdown {
            return;
        }
        let finished = batch.finish(shared);
        let _ = self.context.sender.send(Message::ScanBatchDone(finished));
    }
}
