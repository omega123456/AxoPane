use std::fs;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::names::file::remove_if_present;
use super::names::writer::{
    self, is_past, PairCapture, PairStamp, RewriteError, RewriteStats, WrittenNames,
};
use super::names::InstallError;
use super::persist::{self, SnapshotMeta, NO_PAIR};

use super::machine::{Machine, Phase};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveTrigger {
    Blur,
    CatchUp,
    ExclusionEdit,
    ScanComplete,
    Exit(Instant),
}

impl SaveTrigger {
    fn deadline(self) -> Option<Instant> {
        match self {
            Self::Exit(deadline) => Some(deadline),
            Self::Blur | Self::CatchUp | Self::ExclusionEdit | Self::ScanComplete => None,
        }
    }

    fn saves_every_name_change(self) -> bool {
        matches!(
            self,
            Self::ExclusionEdit | Self::ScanComplete | Self::Exit(_)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveKind {
    Skip,
    FolderOnly,
    Pair,
}

#[derive(Debug, Clone, Copy)]
struct SaveState {
    structure_changed: bool,
    names_unsaved: bool,
    overlay_over_threshold: bool,
    folders_dirty: bool,
}

fn choose_save(trigger: SaveTrigger, state: SaveState) -> SaveKind {
    if state.structure_changed {
        return SaveKind::Pair;
    }
    if names_due(trigger, state) {
        return SaveKind::Pair;
    }
    if state.folders_dirty {
        return SaveKind::FolderOnly;
    }
    SaveKind::Skip
}

fn names_due(trigger: SaveTrigger, state: SaveState) -> bool {
    if trigger.saves_every_name_change() {
        return state.names_unsaved;
    }
    state.overlay_over_threshold
}

fn next_pair_token(previous: u64) -> u64 {
    let candidate = RandomState::new().hash_one(previous);
    let usable = candidate != previous && candidate != NO_PAIR;
    if usable {
        return candidate;
    }
    previous.wrapping_add(1).max(NO_PAIR + 1)
}

struct PairSave {
    event_id: u64,
    trigger: SaveTrigger,
}

struct PairInputs {
    snapshot: Vec<u8>,
    names: PairCapture,
}

struct WrittenPair {
    names: WrittenNames,
    snapshot_temp: PathBuf,
    meta: SnapshotMeta,
    started: Instant,
}

struct PairReport {
    stats: RewriteStats,
    started: Instant,
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
        let event_id = self.snapshot_event_id();
        if !self.shared.collect_file_names {
            return self.save_folders_without_names(event_id);
        }
        match choose_save(trigger, self.save_state(event_id)) {
            SaveKind::Skip => {}
            SaveKind::FolderOnly => {
                self.save_folder_snapshot(event_id);
            }
            SaveKind::Pair => self.save_pair(PairSave { event_id, trigger }),
        }
    }

    fn save_state(&self, event_id: u64) -> SaveState {
        let view = self.shared.view();
        let no_pair_on_disk = self.meta.pair_token == NO_PAIR || !view.names.has_names_file();
        SaveState {
            structure_changed: view.tree.structure_changed() || no_pair_on_disk,
            names_unsaved: view.names.has_unsaved_names(),
            overlay_over_threshold: view.names.overlay_stats().exceeds_threshold(),
            folders_dirty: view.tree.is_dirty()
                || self.meta_dirty
                || event_id != self.meta.event_id,
        }
    }

    fn next_meta(&self, event_id: u64, pair_token: u64) -> SnapshotMeta {
        let mut visited: Vec<PathBuf> = self.shared.view().visited.iter().cloned().collect();
        visited.sort();
        SnapshotMeta {
            event_id,
            pair_token,
            visited_roots: visited,
            ..self.meta.clone()
        }
    }

    fn save_folders_without_names(&mut self, event_id: u64) {
        if !self.folders_unsaved(event_id) {
            return;
        }
        let saved = self.save_folder_snapshot(event_id);
        if saved {
            self.shared.view_mut().tree.mark_structure_saved();
        }
    }

    fn folders_unsaved(&self, event_id: u64) -> bool {
        let view = self.shared.view();
        view.tree.is_dirty()
            || view.tree.structure_changed()
            || self.meta_dirty
            || event_id != self.meta.event_id
    }

    fn save_folder_snapshot(&mut self, event_id: u64) -> bool {
        let started = Instant::now();
        let meta = self.next_meta(event_id, self.meta.pair_token);
        let bytes = persist::encode(&meta, &self.shared.view().tree);
        if let Err(error) = persist::save_encoded(&self.snapshot_path, &bytes) {
            log::warn!(
                "size index: snapshot write to {} failed: {error}",
                self.snapshot_path.display()
            );
            return false;
        }
        self.shared.view_mut().tree.mark_clean();
        self.meta = meta;
        self.meta_dirty = false;
        log::info!(
            "size index: folder snapshot saved ({} bytes) in {}ms",
            bytes.len(),
            started.elapsed().as_millis()
        );
        true
    }

    fn capture_pair(&self, meta: &SnapshotMeta) -> PairInputs {
        let view = self.shared.view();
        PairInputs {
            snapshot: persist::encode(meta, &view.tree),
            names: view.names.capture(view.tree.export_slots()),
        }
    }

    fn save_pair(&mut self, save: PairSave) {
        let started = Instant::now();
        let deadline = save.trigger.deadline();
        let meta = self.next_meta(save.event_id, next_pair_token(self.meta.pair_token));
        let inputs = self.capture_pair(&meta);
        log::info!(
            "size index: pair save started ({:?}, {} folders)",
            save.trigger,
            inputs.names.order.len()
        );
        let stamp = PairStamp {
            event_id: save.event_id,
            pair_token: meta.pair_token,
            deadline,
        };
        let names = match writer::write_names(inputs.names, stamp) {
            Ok(names) => names,
            Err(error) => return self.pair_write_failed(error),
        };
        let snapshot_temp = match persist::write_temp(&self.snapshot_path, &inputs.snapshot) {
            Ok(temp) => temp,
            Err(error) => return discard_written_names(&names, &error.to_string()),
        };
        if is_past(deadline) {
            let _ = remove_if_present(&snapshot_temp);
            return discard_written_names(&names, "exit deadline passed");
        }
        self.install_pair(WrittenPair {
            names,
            snapshot_temp,
            meta,
            started,
        });
    }

    fn pair_write_failed(&mut self, error: RewriteError) {
        match error {
            RewriteError::Unreadable => self.names_unreadable(),
            RewriteError::DeadlinePassed => {
                log::warn!("size index: pair save skipped, the exit deadline passed");
            }
            RewriteError::Io(error) => log::warn!("size index: file-name write failed: {error}"),
        }
    }

    fn install_pair(&mut self, pair: WrittenPair) {
        let WrittenPair {
            names,
            snapshot_temp,
            meta,
            started,
        } = pair;
        let report = PairReport {
            stats: names.stats,
            started,
        };
        let installed = self.install_under_lock(names, &snapshot_temp);
        let _ = remove_if_present(&snapshot_temp);
        match installed {
            Ok(()) => self.pair_installed(meta, &report),
            Err(error) => self.pair_install_failed(error),
        }
    }

    fn install_under_lock(
        &self,
        names: WrittenNames,
        snapshot_temp: &Path,
    ) -> Result<(), InstallError> {
        let mut view = self.shared.view_mut();
        let installed = view
            .names
            .install(names, || fs::rename(snapshot_temp, &self.snapshot_path));
        if installed.is_ok() {
            view.tree.mark_clean();
            view.tree.mark_structure_saved();
        }
        installed
    }

    fn pair_installed(&mut self, meta: SnapshotMeta, report: &PairReport) {
        log::info!(
            "size index: pair saved in {}ms ({} blocks re-encoded, {} copied)",
            report.started.elapsed().as_millis(),
            report.stats.blocks_reencoded,
            report.stats.blocks_copied
        );
        self.meta = meta;
        self.meta_dirty = false;
        self.restart_spill_if_scanning();
    }

    fn pair_install_failed(&mut self, error: InstallError) {
        match error {
            InstallError::NamesNotReplaced(error) => {
                log::warn!("size index: file-name file replace failed: {error}");
            }
            InstallError::SnapshotNotReplaced(error) => {
                self.meta.pair_token = NO_PAIR;
                log::warn!("size index: snapshot replace failed after file names: {error}");
            }
        }
    }
}

fn discard_written_names(names: &WrittenNames, reason: &str) {
    let _ = remove_if_present(&names.temp);
    log::warn!("size index: pair save abandoned: {reason}");
}
