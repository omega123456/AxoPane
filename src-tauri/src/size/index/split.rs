use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::size::fast_walk::{DeviceBoundary, DirListing, DirRecord, WalkOptions};
use crate::size::manual::ManualSizeError;

use super::lanes::{
    narrow_walk, send_records, BackgroundTurn, Gentle, LaneContext, LaneWalk, WalkOutcome,
};
use super::machine::Message;

pub(super) const LISTED_LEVELS: usize = 3;

#[derive(Default)]
struct ListedLevels {
    records: Vec<DirRecord>,
    tops: Vec<PathBuf>,
}

struct LevelListing<'split> {
    lane_walk: &'split LaneWalk<'split>,
    turn: BackgroundTurn<'split>,
    gentle: Gentle,
    options: WalkOptions,
    boundary: DeviceBoundary,
}

fn record_of(dir: &Path, listing: DirListing) -> DirRecord {
    DirRecord {
        path: dir.to_path_buf(),
        direct_bytes: listing.direct_bytes,
        file_count: listing.file_count,
        subdirs: listing.subdirs,
        denied: listing.denied,
        file_names: listing.file_names,
    }
}

impl LevelListing<'_> {
    fn list_levels(&mut self) -> Result<ListedLevels, ManualSizeError> {
        let mut listed = ListedLevels::default();
        let mut level = vec![self.lane_walk.root.to_path_buf()];
        for _ in 0..LISTED_LEVELS {
            level = self.list_level(level, &mut listed.records)?;
        }
        listed.tops = level.into_iter().filter(|top| self.walks(top)).collect();
        Ok(listed)
    }

    fn list_level(
        &mut self,
        level: Vec<PathBuf>,
        records: &mut Vec<DirRecord>,
    ) -> Result<Vec<PathBuf>, ManualSizeError> {
        let mut next_level = Vec::new();
        for dir in level {
            next_level.extend(self.list_dir(&dir, records)?);
        }
        Ok(next_level)
    }

    fn list_dir(
        &mut self,
        dir: &Path,
        records: &mut Vec<DirRecord>,
    ) -> Result<Vec<PathBuf>, ManualSizeError> {
        let Some(record) = self.record(dir)? else {
            return Ok(Vec::new());
        };
        let subdirs = record.subdirs.iter().map(|name| dir.join(name)).collect();
        records.push(record);
        self.gentle.after_record(self.turn);
        Ok(subdirs)
    }

    fn record(&self, dir: &Path) -> Result<Option<DirRecord>, ManualSizeError> {
        if self.lane_walk.cancel.load(Ordering::Relaxed) {
            return Err(ManualSizeError::Cancelled);
        }
        if self.options.excluded.contains(dir) {
            return Ok(None);
        }
        let is_root = dir == self.lane_walk.root;
        match self.boundary.list_dir(dir, &self.options) {
            Ok(listing) => Ok(Some(record_of(dir, listing))),
            Err(ManualSizeError::Cancelled) => Err(ManualSizeError::Cancelled),
            Err(error) if is_root => Err(error),
            Err(_) => Ok(None),
        }
    }

    fn walks(&self, top: &Path) -> bool {
        let is_excluded = self.options.excluded.contains(top);
        !is_excluded && self.boundary.holds(top, &self.options)
    }
}

pub(super) fn split_root(context: LaneContext<'_>, lane_walk: LaneWalk<'_>) -> WalkOutcome {
    let LaneContext { shared, sender } = context;
    let mut plan = lane_walk.lane.plan(shared, lane_walk.cancel);
    if !narrow_walk(shared, lane_walk.root, &mut plan) {
        return WalkOutcome::Skipped;
    }
    let boundary = match DeviceBoundary::of(lane_walk.root, &plan.options) {
        Ok(boundary) => boundary,
        Err(error) => return WalkOutcome::Failed(error),
    };
    let _ = sender.send(Message::WalkStarted {
        root: lane_walk.root.to_path_buf(),
        lane: lane_walk.lane,
    });
    let mut listing = LevelListing {
        lane_walk: &lane_walk,
        turn: BackgroundTurn {
            shared,
            cancel: lane_walk.cancel,
        },
        gentle: Gentle::admit(shared, lane_walk.cancel),
        options: plan.options,
        boundary,
    };
    let listed = match listing.list_levels() {
        Ok(listed) => listed,
        Err(error) => return WalkOutcome::Failed(error),
    };
    send_records(context, listed.records, true);
    let _ = sender.send(Message::ScanTops(listed.tops));
    WalkOutcome::Split
}
