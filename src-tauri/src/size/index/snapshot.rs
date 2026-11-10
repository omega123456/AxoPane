use std::path::PathBuf;
use std::time::Instant;

use super::persist::{self, Manifest, SnapshotMeta};

use super::machine::{Machine, Phase};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveTrigger {
    Blur,
    CatchUp,
    ExclusionEdit,
    ScanComplete,
    Exit,
}

impl SaveTrigger {
    fn rewrites_the_base(self) -> bool {
        matches!(self, Self::ExclusionEdit | Self::ScanComplete)
    }

    fn saves_before_compacting(self) -> bool {
        self == Self::ScanComplete
    }
}

impl Machine {
    fn holds_saved_event_id(&self) -> bool {
        self.background_inflight > 0 || matches!(self.phase, Phase::ReplayingHistory(_))
    }

    fn snapshot_event_id(&self) -> u64 {
        if let Some(full) = &self.full {
            return full.captured_id;
        }
        if self.holds_saved_event_id() {
            return self.meta.event_id;
        }
        self.last_applied_id
    }

    pub(super) fn save_snapshot(&mut self, trigger: SaveTrigger) {
        if self.stream.is_none() {
            return;
        }
        if matches!(self.phase, Phase::Booting) {
            return;
        }
        if trigger.saves_before_compacting() {
            self.save_manifest();
        }
        if trigger.rewrites_the_base() {
            return self.start_compaction();
        }
        if self.has_unsaved_changes() {
            self.save_manifest();
        }
    }

    fn has_unsaved_changes(&self) -> bool {
        let view = self.shared.view();
        let index_changed = view.tree.is_dirty() || view.names.has_unsaved_changes();
        let meta_changed = self.meta_dirty || self.snapshot_event_id() != self.meta.event_id;
        index_changed || meta_changed
    }

    fn next_meta(&self) -> SnapshotMeta {
        let mut visited: Vec<PathBuf> = self.shared.view().visited.iter().cloned().collect();
        visited.sort();
        SnapshotMeta {
            event_id: self.snapshot_event_id(),
            visited_roots: visited,
            has_file_names: self.shared.collect_file_names,
            names_purge_due: self.purge_saved_names,
            full_scan_running: self.full.is_some(),
            ..self.meta.clone()
        }
    }

    fn write_manifest(&self, meta: &SnapshotMeta) -> std::io::Result<usize> {
        let view = self.shared.view();
        let bytes = persist::encode(&Manifest {
            meta,
            segments: &self.compactor.segments,
            tree: &view.tree,
            names: &view.names,
        });
        drop(view);
        persist::save_encoded(&self.snapshot_path, &bytes)?;
        Ok(bytes.len())
    }

    pub(super) fn save_manifest(&mut self) -> bool {
        let started = Instant::now();
        let meta = self.next_meta();
        let manifest_bytes = match self.write_manifest(&meta) {
            Ok(manifest_bytes) => manifest_bytes,
            Err(error) => {
                log::warn!(
                    "size index: manifest write to {} failed: {error}",
                    self.snapshot_path.display()
                );
                return false;
            }
        };
        let mut view = self.shared.view_mut();
        view.tree.mark_clean();
        view.tree.mark_structure_saved();
        view.names.mark_saved();
        drop(view);
        self.meta = meta;
        self.meta_dirty = false;
        self.sweep_unreferenced_files();
        log::info!(
            "size index: manifest saved ({manifest_bytes} bytes) in {}ms",
            started.elapsed().as_millis()
        );
        true
    }
}
