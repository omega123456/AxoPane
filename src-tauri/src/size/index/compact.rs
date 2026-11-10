use std::fs;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};

use super::machine::{Machine, Message};
use super::names::file::remove_if_present;
use super::names::SavedNames;
use super::persist::SegmentRef;
use super::segment::{
    sequence_of_segment_file, BaseRewrite, BuildError, BuiltSegment, MappedSegment, RewriteFailure,
    RewriteProgress, RewriteSource, SegmentBuilder, SegmentPaths, SegmentTarget,
};
use super::service::Shared;
use super::tree::FolderSizeTree;

const FIRST_SEQUENCE: u64 = 1;

#[derive(Debug, Clone)]
pub(super) enum StopSignal {
    Cancelled(Arc<AtomicBool>),
    Deadline(Instant),
}

impl StopSignal {
    fn is_raised(&self) -> bool {
        match self {
            Self::Cancelled(cancelled) => cancelled.load(Ordering::Relaxed),
            Self::Deadline(deadline) => Instant::now() >= *deadline,
        }
    }
}

pub(super) struct CompactJob {
    pub(super) sequence: u64,
    pub(super) target: SegmentTarget,
    pub(super) stop: StopSignal,
}

#[derive(Debug)]
pub(super) enum CompactError {
    Stopped,
    NamesUnreadable,
    Build(BuildError),
}

impl From<BuildError> for CompactError {
    fn from(error: BuildError) -> Self {
        Self::Build(error)
    }
}

impl From<RewriteFailure> for CompactError {
    fn from(failure: RewriteFailure) -> Self {
        match failure {
            RewriteFailure::NamesUnreadable => Self::NamesUnreadable,
            RewriteFailure::Build(error) => Self::Build(error),
        }
    }
}

pub(super) struct FinishedCompaction {
    pub(super) sequence: u64,
    pub(super) result: Result<BuiltSegment, CompactError>,
}

fn compact(shared: &Shared, job: &CompactJob) -> Result<BuiltSegment, CompactError> {
    let mut builder = SegmentBuilder::create(job.target.clone())?;
    if let Err(error) = copy_folders(shared, &job.stop, &mut builder) {
        builder.discard();
        return Err(error);
    }
    Ok(builder.finish()?)
}

fn copy_folders(
    shared: &Shared,
    stop: &StopSignal,
    builder: &mut SegmentBuilder,
) -> Result<(), CompactError> {
    let mut rewrite = BaseRewrite::of(&shared.view().tree);
    let mut progress = RewriteProgress::MoreFolders;
    while progress == RewriteProgress::MoreFolders {
        if stop.is_raised() {
            return Err(CompactError::Stopped);
        }
        let view = shared.view();
        let source = RewriteSource {
            tree: &view.tree,
            names: &view.names,
        };
        progress = rewrite.copy_step(source, builder)?;
    }
    Ok(())
}

pub(super) fn run_compactions(
    shared: &Shared,
    jobs: &Receiver<CompactJob>,
    sender: &Sender<Message>,
) {
    for job in jobs {
        let finished = FinishedCompaction {
            sequence: job.sequence,
            result: compact(shared, &job),
        };
        let _ = sender.send(Message::Compacted(finished));
    }
}

pub(super) struct RunningCompaction {
    sequence: u64,
    cancel: Arc<AtomicBool>,
    deferred: Vec<Message>,
}

pub(super) struct Compactor {
    jobs: Sender<CompactJob>,
    directory: PathBuf,
    running: Option<RunningCompaction>,
    pub(super) segments: Vec<SegmentRef>,
    next_sequence: u64,
}

impl Compactor {
    pub(super) fn new(jobs: Sender<CompactJob>, snapshot_path: &Path) -> Self {
        let directory = snapshot_path.parent().unwrap_or(snapshot_path);
        Self {
            jobs,
            directory: directory.to_path_buf(),
            running: None,
            segments: Vec::new(),
            next_sequence: FIRST_SEQUENCE,
        }
    }

    pub(super) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn defer(&mut self, message: Message) -> Option<Message> {
        let Some(running) = &mut self.running else {
            return Some(message);
        };
        running.deferred.push(message);
        None
    }

    pub(super) fn cancel_running(&mut self) -> Vec<Message> {
        let Some(running) = self.running.take() else {
            return Vec::new();
        };
        running.cancel.store(true, Ordering::Relaxed);
        running.deferred
    }

    fn take_if_current(&mut self, sequence: u64) -> Option<RunningCompaction> {
        self.running.take_if(|running| running.sequence == sequence)
    }

    fn next_job(&mut self, stop: StopSignal) -> CompactJob {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        CompactJob {
            sequence,
            target: SegmentTarget {
                paths: SegmentPaths::numbered(&self.directory, sequence),
                token: RandomState::new().hash_one(sequence),
            },
            stop,
        }
    }

    fn start(&mut self) {
        let cancel = Arc::new(AtomicBool::new(false));
        let job = self.next_job(StopSignal::Cancelled(Arc::clone(&cancel)));
        self.running = Some(RunningCompaction {
            sequence: job.sequence,
            cancel,
            deferred: Vec::new(),
        });
        let _ = self.jobs.send(job);
    }

    fn is_in_use(&self, sequence: u64) -> bool {
        let is_referenced = self
            .segments
            .iter()
            .any(|segment| segment.sequence == sequence);
        let is_being_built = self
            .running
            .as_ref()
            .is_some_and(|running| running.sequence == sequence);
        is_referenced || is_being_built
    }

    fn segment_files(&self) -> Vec<SegmentFile> {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let sequence = sequence_of_segment_file(&entry.file_name().to_string_lossy())?;
                Some(SegmentFile {
                    path: entry.path(),
                    sequence,
                })
            })
            .collect()
    }

    pub(super) fn skip_sequences_on_disk(&mut self) {
        let used = self.segment_files().into_iter().map(|file| file.sequence);
        let after_last_used = used.max().map_or(FIRST_SEQUENCE, |last| last + 1);
        self.next_sequence = self.next_sequence.max(after_last_used);
    }

    pub(super) fn sweep_unreferenced_files(&self) {
        let unreferenced = self
            .segment_files()
            .into_iter()
            .filter(|file| !self.is_in_use(file.sequence));
        for file in unreferenced {
            remove_segment_file(&file.path);
        }
    }
}

struct SegmentFile {
    path: PathBuf,
    sequence: u64,
}

fn remove_segment_file(path: &Path) {
    if let Err(error) = remove_if_present(path) {
        log::warn!(
            "size index: unreferenced segment file {} could not be removed: {error}",
            path.display()
        );
    }
}

fn discard_built(result: Result<BuiltSegment, CompactError>) {
    let Ok(built) = result else {
        return;
    };
    remove_segment_file(&built.paths.tree);
    remove_segment_file(&built.paths.names);
}

struct NewBase {
    sequence: u64,
    built: BuiltSegment,
}

impl Machine {
    pub(super) fn start_compaction(&mut self) {
        if self.compactor.is_running() {
            return;
        }
        log::info!("size index: compaction started");
        self.compactor.start();
    }

    pub(super) fn compact_before(&mut self, deadline: Instant) {
        let job = self.compactor.next_job(StopSignal::Deadline(deadline));
        let finished = FinishedCompaction {
            sequence: job.sequence,
            result: compact(&self.shared, &job),
        };
        self.finish_compaction(finished);
    }

    pub(super) fn cancel_compaction(&mut self) {
        for message in self.compactor.cancel_running() {
            self.handle(message);
        }
    }

    pub(super) fn stop_compaction(&mut self) {
        self.compactor.cancel_running();
    }

    pub(super) fn compacted(&mut self, finished: FinishedCompaction) {
        let Some(running) = self.compactor.take_if_current(finished.sequence) else {
            return discard_built(finished.result);
        };
        self.finish_compaction(finished);
        for message in running.deferred {
            self.handle(message);
        }
    }

    fn finish_compaction(&mut self, finished: FinishedCompaction) {
        let FinishedCompaction { sequence, result } = finished;
        match result {
            Ok(built) => self.install_base(NewBase { sequence, built }),
            Err(CompactError::Stopped) => log::info!("size index: compaction stopped early"),
            Err(CompactError::NamesUnreadable) => self.names_unreadable(),
            Err(CompactError::Build(error)) => {
                log::warn!("size index: compaction failed: {error:?}");
            }
        }
    }

    fn install_base(&mut self, base: NewBase) {
        let NewBase { sequence, built } = base;
        let Some(segment) = MappedSegment::open(&built.paths) else {
            log::warn!("size index: the compacted segment could not be opened");
            return discard_built(Ok(built));
        };
        let token = segment.token();
        let file_names = segment.shared_file_names();
        let mut tree = FolderSizeTree::default();
        let Some(slots) = tree.attach_base(Arc::new(segment)) else {
            log::warn!("size index: the compacted segment could not be attached");
            return discard_built(Ok(built));
        };
        let mut names = SavedNames::default();
        if self.shared.collect_file_names {
            names.attach_segment(slots, file_names);
        }
        log::info!(
            "size index: compacted {} folders into segment {sequence}",
            built.node_count
        );
        let mut view = self.shared.view_mut();
        view.tree = tree;
        view.names.adopt_base(names, built.names);
        drop(view);
        self.compactor.segments = vec![SegmentRef { sequence, token }];
        self.purge_saved_names = false;
        self.meta_dirty = true;
        self.save_manifest();
        self.restart_spill_if_scanning();
    }
}
