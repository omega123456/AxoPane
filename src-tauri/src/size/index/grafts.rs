use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::compact::discard_segment;
use super::exclusions;
use super::lanes::{BuiltGraft, Lane, ScanJob, ScanKind, WalkOutcome};
use super::machine::{FullScan, Machine};
use super::persist::SegmentRef;
use super::scan_batch::{BatchedWalk, BuiltForest, FinishedBatch, ForestOutcome};
use super::segment::BuiltSegment;
use super::segment::MappedSegment;
use super::tree::{FolderSizeTree, Forest, Graft};
use crate::size::manual::ManualSizeError;

pub(super) const REREADS_PER_WALK: usize = 1_024;

pub(super) struct WalkLog {
    roots: Vec<PathBuf>,
    rereads: Vec<PathBuf>,
    overflowed: bool,
}

impl WalkLog {
    fn started(root: PathBuf) -> Self {
        Self {
            roots: vec![root],
            rereads: Vec::new(),
            overflowed: false,
        }
    }

    fn note(&mut self, dir: &Path) {
        let is_under_the_walk = self
            .roots
            .iter()
            .any(|root| exclusions::is_within(dir, root));
        if self.overflowed || !is_under_the_walk {
            return;
        }
        if self.rereads.iter().any(|logged| logged == dir) {
            return;
        }
        if self.rereads.len() >= REREADS_PER_WALK {
            self.overflowed = true;
            self.rereads.clear();
            return;
        }
        self.rereads.push(dir.to_path_buf());
    }
}

#[derive(Default)]
pub(super) struct WalkLogs {
    background: Option<WalkLog>,
    boost: Option<WalkLog>,
}

impl WalkLogs {
    fn of_lane(&mut self, lane: Lane) -> &mut Option<WalkLog> {
        match lane {
            Lane::Background { .. } => &mut self.background,
            Lane::Boost { .. } => &mut self.boost,
        }
    }

    fn note(&mut self, dir: &Path) {
        let running = self.background.iter_mut().chain(self.boost.iter_mut());
        for log in running {
            log.note(dir);
        }
    }
}

struct SegmentPlacement<'built> {
    sequence: u64,
    built: &'built BuiltSegment,
    under: &'built Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WalkEnd {
    Indexed,
    Skipped,
    Cancelled,
    Failed,
}

pub(super) struct FinishedWalk<'walk> {
    pub(super) root: &'walk Path,
    pub(super) lane: Lane,
    pub(super) outcome: WalkOutcome,
}

fn is_not_found(error: &ManualSizeError) -> bool {
    matches!(error, ManualSizeError::Io(io_error) if io_error.kind() == std::io::ErrorKind::NotFound)
}

impl Machine {
    pub(super) fn walk_started(&mut self, root: PathBuf, lane: Lane) {
        let log = self.walk_logs.of_lane(lane);
        let Some(batched) = log else {
            *log = Some(WalkLog::started(root));
            return;
        };
        batched.roots.push(root);
    }

    pub(super) fn note_reread(&mut self, dir: &Path) {
        self.walk_logs.note(dir);
    }

    pub(super) fn end_walk(&mut self, finished: FinishedWalk<'_>) -> WalkEnd {
        let FinishedWalk {
            root,
            lane,
            outcome,
        } = finished;
        let log = self.walk_logs.of_lane(lane).take();
        let end = self.settle_walk(root, outcome);
        if let Some(log) = log {
            self.replay_rereads(log);
        }
        end
    }

    fn settle_walk(&mut self, root: &Path, outcome: WalkOutcome) -> WalkEnd {
        match outcome {
            WalkOutcome::Walked => self.indexed(root),
            WalkOutcome::Built(graft) => self.settle_graft(root, graft),
            WalkOutcome::Split => WalkEnd::Indexed,
            WalkOutcome::Skipped => WalkEnd::Skipped,
            WalkOutcome::Failed(ManualSizeError::Cancelled) => WalkEnd::Cancelled,
            WalkOutcome::Failed(error) if is_not_found(&error) => {
                self.prune(root);
                WalkEnd::Failed
            }
            WalkOutcome::Failed(_) | WalkOutcome::BuildFailed => WalkEnd::Failed,
        }
    }

    fn indexed(&mut self, root: &Path) -> WalkEnd {
        self.shared.view_mut().prune_unscanned(root);
        WalkEnd::Indexed
    }

    fn settle_graft(&mut self, root: &Path, graft: BuiltGraft) -> WalkEnd {
        if !self.install_graft(root, graft) {
            return WalkEnd::Failed;
        }
        self.indexed(root)
    }

    fn install_graft(&mut self, root: &Path, graft: BuiltGraft) -> bool {
        let BuiltGraft {
            sequence,
            built,
            kept,
        } = graft;
        let placement = SegmentPlacement {
            sequence,
            built: &built,
            under: root,
        };
        self.place_segment(placement, |tree, segment| {
            tree.install_graft(Graft {
                path: root,
                segment,
                kept: &kept,
            })
        })
    }

    fn place_segment(
        &mut self,
        placement: SegmentPlacement<'_>,
        attach: impl FnOnce(&mut FolderSizeTree, Arc<MappedSegment>) -> Option<Range<u32>>,
    ) -> bool {
        let SegmentPlacement {
            sequence,
            built,
            under,
        } = placement;
        let attached = MappedSegment::open(&built.paths)
            .and_then(|segment| self.attach_mapped(Arc::new(segment), attach));
        let Some(token) = attached else {
            self.shared.sequences().release(sequence);
            log::warn!(
                "size index: a walk under {} could not be grafted",
                under.display()
            );
            discard_segment(&built.paths);
            return false;
        };
        self.compactor.segments.push(SegmentRef { sequence, token });
        self.shared.sequences().release(sequence);
        self.reskip_protected();
        self.reskip_excluded();
        log::info!(
            "size index: grafted {} folders under {}",
            built.node_count,
            under.display()
        );
        true
    }

    fn attach_mapped(
        &self,
        segment: Arc<MappedSegment>,
        attach: impl FnOnce(&mut FolderSizeTree, Arc<MappedSegment>) -> Option<Range<u32>>,
    ) -> Option<u64> {
        let token = segment.token();
        let file_names = segment.shared_file_names();
        let mut view = self.shared.view_mut();
        let ids = attach(&mut view.tree, segment)?;
        if view.keeps_file_names {
            view.names.attach_segment(ids, file_names);
        }
        view.tombstone_removed();
        Some(token)
    }

    fn requeue_walks(&mut self, roots: Vec<PathBuf>) {
        for root in roots {
            self.queue_background(root);
        }
    }

    fn replay_rereads(&mut self, log: WalkLog) {
        if log.overflowed {
            return self.requeue_walks(log.roots);
        }
        for dir in log.rereads {
            self.reread(&dir);
        }
    }

    pub(super) fn count_scanned(&mut self, items: u64) {
        let scanned_before = self.items_scanned;
        self.items_scanned += items;
        self.log_scan_progress(scanned_before);
    }

    pub(super) fn start_full_scan(&mut self) {
        if self.full.is_some() {
            return;
        }
        self.items_scanned = 0;
        self.full = Some(FullScan {
            captured_id: self.platform.current_event_id(),
            remaining: 0,
            started: Instant::now(),
        });
        let root_jobs = self.shared.roots.iter().map(|root| ScanJob {
            root: root.clone(),
            kind: ScanKind::FullRoot,
        });
        let queued_roots = self.queue_full_jobs(root_jobs.collect());
        log::info!("size index: full scan started with {queued_roots} roots queued");
        self.finish_full_scan_if_done();
    }

    pub(super) fn resume_full_scan(&mut self) {
        self.full = Some(FullScan {
            captured_id: self.meta.event_id,
            remaining: 0,
            started: Instant::now(),
        });
        let tops = self.shared.view().tree.unscanned_tops();
        let queued_tops = self.queue_scan_tops(tops);
        log::info!("size index: resuming the full scan with {queued_tops} subtrees left");
        self.finish_full_scan_if_done();
    }

    pub(super) fn queue_scan_tops(&mut self, tops: Vec<PathBuf>) -> usize {
        let top_jobs = tops.into_iter().map(|root| ScanJob {
            root,
            kind: ScanKind::FullSubtree,
        });
        self.queue_full_jobs(top_jobs.collect())
    }

    fn queue_full_jobs(&mut self, jobs: Vec<ScanJob>) -> usize {
        let queued = jobs
            .into_iter()
            .map(|job| self.shared.queue_background(job))
            .filter(|queued| *queued)
            .count();
        self.background_inflight += queued;
        if let Some(full) = &mut self.full {
            full.remaining += queued;
        }
        queued
    }

    pub(super) fn finish_full_scan_if_done(&mut self) {
        let is_done = self.full.as_ref().is_some_and(|full| full.remaining == 0);
        if is_done {
            self.finish_full_scan();
        }
    }
}

impl Machine {
    pub(super) fn scan_batch_done(&mut self, finished: FinishedBatch) {
        let FinishedBatch { forest, walks } = finished;
        let log = self
            .walk_logs
            .of_lane(Lane::Background { full: true })
            .take();
        let installed = self.install_forest_outcome(forest);
        for walk in walks {
            self.settle_batched_walk(walk, installed);
        }
        if let Some(log) = log {
            self.replay_rereads(log);
        }
        self.finish_full_scan_if_done();
    }

    fn install_forest_outcome(&mut self, forest: ForestOutcome) -> bool {
        match forest {
            ForestOutcome::Built(built) => self.install_forest(built),
            ForestOutcome::Failed => false,
        }
    }

    fn settle_batched_walk(&mut self, walk: BatchedWalk, installed: bool) {
        let BatchedWalk { job, outcome } = walk;
        self.background_inflight = self.background_inflight.saturating_sub(1);
        if let Some(full) = &mut self.full {
            full.remaining = full.remaining.saturating_sub(1);
        }
        let walk_was_lost = !installed && matches!(outcome, WalkOutcome::Walked);
        if walk_was_lost {
            self.queue_background(job.root);
            return;
        }
        self.settle_walk(&job.root, outcome);
    }

    fn install_forest(&mut self, forest: BuiltForest) -> bool {
        let BuiltForest {
            sequence,
            built,
            root,
            tops,
        } = forest;
        let placement = SegmentPlacement {
            sequence,
            built: &built,
            under: &root,
        };
        self.place_segment(placement, |tree, segment| {
            tree.install_forest(Forest {
                root: &root,
                tops: &tops,
                segment,
            })
        })
    }
}
