use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::compact::discard_segment;
use super::exclusions;
use super::lanes::{BuiltGraft, Lane, ScanJob, ScanKind, WalkOutcome};
use super::machine::{FullScan, Machine};
use super::persist::SegmentRef;
use super::segment::MappedSegment;
use super::tree::Graft;
use crate::size::manual::ManualSizeError;

pub(super) const REREADS_PER_WALK: usize = 1_024;

pub(super) struct WalkLog {
    root: PathBuf,
    rereads: Vec<PathBuf>,
    overflowed: bool,
}

impl WalkLog {
    fn started(root: PathBuf) -> Self {
        Self {
            root,
            rereads: Vec::new(),
            overflowed: false,
        }
    }

    fn note(&mut self, dir: &Path) {
        let is_under_the_walk = exclusions::is_within(dir, &self.root);
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
        *self.walk_logs.of_lane(lane) = Some(WalkLog::started(root));
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
        let attached = MappedSegment::open(&built.paths).and_then(|segment| {
            self.attach_graft(Graft {
                path: root,
                segment: Arc::new(segment),
                kept: &kept,
            })
        });
        let Some(token) = attached else {
            self.shared.sequences().release(sequence);
            log::warn!(
                "size index: the walk of {} could not be grafted",
                root.display()
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
            root.display()
        );
        true
    }

    fn attach_graft(&self, graft: Graft<'_>) -> Option<u64> {
        let token = graft.segment.token();
        let file_names = graft.segment.shared_file_names();
        let mut view = self.shared.view_mut();
        let ids = view.tree.install_graft(graft)?;
        if view.keeps_file_names {
            view.names.attach_segment(ids, file_names);
        }
        view.tombstone_removed();
        Some(token)
    }

    fn replay_rereads(&mut self, log: WalkLog) {
        if log.overflowed {
            self.queue_background(log.root);
            return;
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
