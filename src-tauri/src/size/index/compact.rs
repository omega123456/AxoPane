use std::fs;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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

pub(super) const SEGMENT_CAP: usize = 64;

pub(super) struct CompactJob {
    pub(super) sequence: u64,
    pub(super) target: SegmentTarget,
    pub(super) cancel: Arc<AtomicBool>,
}

#[derive(Debug)]
pub(super) enum CompactError {
    Cancelled,
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

pub(super) fn new_segment_target(directory: &Path, sequence: u64) -> SegmentTarget {
    SegmentTarget {
        paths: SegmentPaths::numbered(directory, sequence),
        token: RandomState::new().hash_one(sequence),
    }
}

fn compact(shared: &Shared, job: &CompactJob) -> Result<BuiltSegment, CompactError> {
    let mut builder = SegmentBuilder::create(job.target.clone())?;
    if let Err(error) = copy_folders(shared, &job.cancel, &mut builder) {
        builder.discard();
        return Err(error);
    }
    Ok(builder.finish()?)
}

fn copy_folders(
    shared: &Shared,
    cancel: &AtomicBool,
    builder: &mut SegmentBuilder,
) -> Result<(), CompactError> {
    let mut rewrite = BaseRewrite::of(&shared.view().tree);
    let mut progress = RewriteProgress::MoreFolders;
    while progress == RewriteProgress::MoreFolders {
        if cancel.load(Ordering::Relaxed) {
            return Err(CompactError::Cancelled);
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
    running: Option<RunningCompaction>,
    pub(super) segments: Vec<SegmentRef>,
}

impl Compactor {
    pub(super) fn new(jobs: Sender<CompactJob>) -> Self {
        Self {
            jobs,
            running: None,
            segments: Vec::new(),
        }
    }

    pub(super) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn holds_too_many_segments(&self) -> bool {
        self.segments.len() > SEGMENT_CAP
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

    fn start(&mut self, sequence: u64, directory: &Path) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(RunningCompaction {
            sequence,
            cancel: Arc::clone(&cancel),
            deferred: Vec::new(),
        });
        let _ = self.jobs.send(CompactJob {
            sequence,
            target: new_segment_target(directory, sequence),
            cancel,
        });
    }

    fn references(&self, sequence: u64) -> bool {
        self.segments
            .iter()
            .any(|segment| segment.sequence == sequence)
    }
}

struct SegmentFile {
    path: PathBuf,
    sequence: u64,
}

fn segment_files(directory: &Path) -> Vec<SegmentFile> {
    let Ok(entries) = fs::read_dir(directory) else {
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

fn remove_segment_file(path: &Path) {
    if let Err(error) = remove_if_present(path) {
        log::warn!(
            "size index: unreferenced segment file {} could not be removed: {error}",
            path.display()
        );
    }
}

pub(super) fn discard_segment(paths: &SegmentPaths) {
    remove_segment_file(&paths.tree);
    remove_segment_file(&paths.names);
}

fn discard_built(result: Result<BuiltSegment, CompactError>) {
    if let Ok(built) = result {
        discard_segment(&built.paths);
    }
}

struct NewBase {
    sequence: u64,
    built: BuiltSegment,
}

impl Machine {
    pub(super) fn skip_sequences_on_disk(&self) {
        let used = segment_files(&self.shared.segment_directory)
            .into_iter()
            .map(|file| file.sequence);
        if let Some(last_used) = used.max() {
            self.shared.sequences().skip_past(last_used);
        }
    }

    pub(super) fn sweep_unreferenced_files(&self) {
        let sequences = self.shared.sequences();
        let unreferenced = segment_files(&self.shared.segment_directory)
            .into_iter()
            .filter(|file| !self.compactor.references(file.sequence))
            .filter(|file| !sequences.is_building(file.sequence));
        for file in unreferenced {
            remove_segment_file(&file.path);
        }
    }

    pub(super) fn start_compaction(&mut self) {
        if self.compactor.is_running() {
            return;
        }
        log::info!("size index: compaction started");
        let sequence = self.shared.sequences().claim();
        self.compactor
            .start(sequence, &self.shared.segment_directory);
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
        self.shared.sequences().release(finished.sequence);
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
            Err(CompactError::Cancelled) => log::info!("size index: compaction cancelled"),
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
            return discard_segment(&built.paths);
        };
        let token = segment.token();
        let file_names = segment.shared_file_names();
        let mut tree = FolderSizeTree::default();
        let Some(slots) = tree.attach_base(Arc::new(segment)) else {
            log::warn!("size index: the compacted segment could not be attached");
            return discard_segment(&built.paths);
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
    }
}
