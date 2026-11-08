use std::collections::HashSet;
use std::path::PathBuf;

use crate::size::SizeUpdate;

use super::machine::Machine;

impl Machine {
    pub(super) fn subscribed(&self) -> Vec<(String, PathBuf)> {
        let mut seen = HashSet::new();
        self.subscriptions
            .values()
            .flatten()
            .filter(|(path, _)| seen.insert(path.as_str()))
            .cloned()
            .collect()
    }

    fn replace_pane_subscriptions(&mut self, pane: String, folders: Vec<(String, PathBuf)>) {
        if folders.is_empty() {
            self.subscriptions.remove(&pane);
            return;
        }
        self.subscriptions.insert(pane, folders);
    }

    /// Replaces a pane's set; newly subscribed folders get their current
    /// indexed totals, so a revisited folder is never stale.
    pub(super) fn subscribe(&mut self, pane: String, folders: Vec<(String, PathBuf)>) {
        let before: HashSet<String> = self
            .subscribed()
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        self.replace_pane_subscriptions(pane, folders);
        let view = self.shared.view();
        let updates: Vec<SizeUpdate> = self
            .subscribed()
            .into_iter()
            .filter(|(path, _)| !before.contains(path))
            .filter_map(|(path, normalized)| self.shared.settled_update(&view, &path, &normalized))
            .collect();
        drop(view);
        if !updates.is_empty() {
            (self.emit_sizes)(updates);
        }
    }

    /// After every message: emit changed subscribed totals (coalesced per
    /// batch) and the status.
    pub(super) fn after_change(&mut self) {
        let subscribed = self.subscribed();
        let updates = self.changed_subscribed_updates(&subscribed);
        if !updates.is_empty() {
            (self.emit_sizes)(updates);
        }
        self.publish_status();
    }

    fn changed_subscribed_updates(&self, subscribed: &[(String, PathBuf)]) -> Vec<SizeUpdate> {
        let mut view = self.shared.view_mut();
        let changed: HashSet<PathBuf> = view
            .tree
            .drain_changed(subscribed.iter().map(|(_, normalized)| normalized))
            .into_iter()
            .collect();
        subscribed
            .iter()
            .filter(|(_, normalized)| changed.contains(normalized))
            .filter_map(|(path, normalized)| self.shared.settled_update(&view, path, normalized))
            .collect()
    }
}
