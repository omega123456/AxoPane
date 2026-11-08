use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use super::machine::{Machine, Phase};
use super::service::{View, WEEK};

impl Machine {
    /// Startup privacy state: unvisited protected roots are skipped (and
    /// stale data under them dropped) unless Full Disk Access is granted.
    pub(super) fn apply_privacy(&mut self) {
        let mut view = self.shared.view_mut();
        for root in &self.shared.protected {
            view.apply_protection(root);
        }
    }

    /// A protected root that appeared in a listing while unvisited is
    /// skipped, so walks never reach it and ancestors report a bound.
    pub(super) fn reskip_protected(&mut self) {
        let mut view = self.shared.view_mut();
        if view.full_disk_access {
            return;
        }
        let reskippable: Vec<&PathBuf> = self
            .shared
            .protected
            .iter()
            .filter(|root| is_unvisited_and_unread(&view, root))
            .collect();
        for root in reskippable {
            view.tree.set_skipped(root, true);
        }
    }

    pub(super) fn visited(&mut self, root: PathBuf) {
        self.shared.view_mut().tree.set_skipped(&root, false);
        self.meta_dirty = true;
        self.queue_background(root);
    }

    pub(super) fn focused(&mut self) {
        let granted = self.platform.full_disk_access();
        for root in self.update_full_disk_access(granted) {
            self.queue_background(root);
        }
        if matches!(self.phase, Phase::Live) && self.stream.is_some() && self.weekly_due() {
            self.start_full_scan();
        }
    }

    /// Returns the protected roots a fresh grant opens up.
    fn update_full_disk_access(&self, granted: bool) -> Vec<PathBuf> {
        let mut view = self.shared.view_mut();
        let newly_granted = granted && !view.full_disk_access;
        view.full_disk_access = granted;
        if !newly_granted {
            return Vec::new();
        }
        let lifted = self.shared.unvisited_protected(&view);
        for root in &lifted {
            view.tree.set_skipped(root, false);
        }
        lifted
    }

    pub(super) fn weekly_due(&self) -> bool {
        let last = UNIX_EPOCH + Duration::from_secs(self.meta.last_full_scan_secs);
        (self.clock)()
            .duration_since(last)
            .is_ok_and(|age| age > WEEK)
    }
}

impl View {
    fn apply_protection(&mut self, root: &Path) {
        let blocked = !self.full_disk_access && !self.visited.contains(root);
        if blocked {
            self.block_protected_root(root);
            return;
        }
        self.unblock_protected_root(root);
    }

    fn block_protected_root(&mut self, root: &Path) {
        if self.tree.total(root).is_some_and(|total| total.scanned) {
            self.tree.prune(root);
        }
        if root.symlink_metadata().is_ok_and(|meta| meta.is_dir()) {
            self.tree.set_skipped(root, true);
        }
    }

    fn unblock_protected_root(&mut self, root: &Path) {
        if self.tree.total(root).is_some_and(|total| total.skipped) {
            self.tree.set_skipped(root, false);
        }
    }
}

fn is_unvisited_and_unread(view: &View, root: &Path) -> bool {
    !view.visited.contains(root)
        && view
            .tree
            .total(root)
            .is_some_and(|total| !total.skipped && !total.scanned)
}
