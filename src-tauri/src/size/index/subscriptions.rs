use std::collections::HashSet;
use std::path::PathBuf;

use crate::size::SizeUpdate;

use super::machine::Machine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubscribedFolder {
    pub(crate) path: String,
    pub(crate) normalized: PathBuf,
}

impl Machine {
    pub(super) fn subscribed(&self) -> Vec<SubscribedFolder> {
        let mut seen = HashSet::new();
        self.subscriptions
            .values()
            .flatten()
            .filter(|folder| seen.insert(folder.path.as_str()))
            .cloned()
            .collect()
    }

    fn replace_pane_subscriptions(&mut self, pane: String, folders: Vec<SubscribedFolder>) {
        if folders.is_empty() {
            self.subscriptions.remove(&pane);
            return;
        }
        self.subscriptions.insert(pane, folders);
    }

    pub(super) fn subscribe(&mut self, pane: String, folders: Vec<SubscribedFolder>) {
        let before: HashSet<String> = self
            .subscribed()
            .into_iter()
            .map(|folder| folder.path)
            .collect();
        self.replace_pane_subscriptions(pane, folders);
        let view = self.shared.view();
        let updates: Vec<SizeUpdate> = self
            .subscribed()
            .into_iter()
            .filter(|folder| !before.contains(&folder.path))
            .filter_map(|folder| self.shared.settled_update(&view, &folder))
            .collect();
        drop(view);
        if !updates.is_empty() {
            (self.emit_sizes)(updates);
        }
    }

    pub(super) fn after_change(&mut self) {
        let subscribed = self.subscribed();
        let updates = self.changed_subscribed_updates(&subscribed);
        if !updates.is_empty() {
            (self.emit_sizes)(updates);
        }
        self.publish_status();
    }

    fn changed_subscribed_updates(&self, subscribed: &[SubscribedFolder]) -> Vec<SizeUpdate> {
        let mut view = self.shared.view_mut();
        let changed: HashSet<PathBuf> = view
            .tree
            .drain_changed(subscribed.iter().map(|folder| &folder.normalized))
            .into_iter()
            .collect();
        subscribed
            .iter()
            .filter(|folder| changed.contains(&folder.normalized))
            .filter_map(|folder| self.shared.settled_update(&view, folder))
            .collect()
    }
}
