use std::path::PathBuf;

use super::persist::{self};

use super::machine::{Machine, Phase};

impl Machine {
    /// Writes the snapshot if anything changed. The saved event ID
    /// is the pre-scan one while a full scan runs, and stays put while
    /// subtree scans are in flight, so a relaunch replays what they cover.
    pub(super) fn save_snapshot(&mut self) {
        if self.stream.is_none() || matches!(self.phase, Phase::Booting) {
            return;
        }
        let event_id = match &self.full {
            Some(full) => full.captured_id,
            None if self.background_inflight > 0
                || matches!(self.phase, Phase::ReplayingHistory(_)) =>
            {
                self.meta.event_id
            }
            None => self.last_applied_id,
        };
        let mut view = self.shared.view_mut();
        if !view.tree.is_dirty() && !self.meta_dirty && event_id == self.meta.event_id {
            return;
        }
        self.meta.event_id = event_id;
        let mut visited: Vec<PathBuf> = view.visited.iter().cloned().collect();
        visited.sort();
        self.meta.visited_roots = visited;
        match persist::save(&self.snapshot_path, &self.meta, &view.tree) {
            Ok(()) => {
                view.tree.mark_clean();
                self.meta_dirty = false;
            }
            Err(error) => log::warn!(
                "size index: snapshot write to {} failed: {error}",
                self.snapshot_path.display()
            ),
        }
    }
}
