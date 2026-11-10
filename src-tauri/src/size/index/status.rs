use super::platform::PowerSignal;
use crate::size::{FolderSizeIndexStatus, IndexBackend, IndexState, PauseReason};

use super::machine::{Machine, Phase};
use super::service::PROGRESS_INTERVAL;

impl Machine {
    pub(super) fn power(&mut self, signal: PowerSignal) {
        match signal {
            PowerSignal::WillSleep => self.sleeping = true,
            PowerSignal::DidWake => self.sleeping = false,
            PowerSignal::Conditions {
                low_power,
                thermal_pressure,
            } => {
                self.low_power = low_power;
                self.thermal_pressure = thermal_pressure;
            }
        }
        self.shared.lanes().power_paused = self.sleeping || self.low_power || self.thermal_pressure;
        self.shared.wake.notify_all();
    }

    fn pause_reason(&self) -> Option<PauseReason> {
        if self.low_power {
            return Some(PauseReason::LowPowerMode);
        }
        if self.thermal_pressure {
            return Some(PauseReason::Thermal);
        }
        None
    }

    pub(super) fn compute_status(&self) -> FolderSizeIndexStatus {
        let stream_lost = self.stream.is_none() && !matches!(self.phase, Phase::Booting);
        if stream_lost {
            return FolderSizeIndexStatus::new(
                IndexBackend::MacIndex,
                false,
                IndexState::Unavailable,
            );
        }
        let mut status =
            FolderSizeIndexStatus::new(IndexBackend::MacIndex, true, IndexState::NotReady);
        let background = self.full.is_some() || self.background_inflight > 0;
        let rewriting_the_base = self.compactor.is_running();
        status.state = match (&self.phase, self.pause_reason()) {
            (Phase::Booting, _) => IndexState::NotReady,
            (_, Some(reason)) if background => {
                status.pause_reason = Some(reason);
                IndexState::Paused
            }
            _ if self.full.is_some() => {
                status.items_scanned = Some(self.items_scanned);
                IndexState::Indexing
            }
            (Phase::ReplayingHistory(_), _) => IndexState::Updating,
            _ if background || rewriting_the_base => IndexState::Updating,
            _ => IndexState::UpToDate,
        };
        status.file_names_available = self.shared.view().names.is_available();
        status
    }

    pub(super) fn republish_status(&mut self) {
        let current = self.compute_status();
        *self.shared.status.lock().expect("index status lock") = current.clone();
        (self.emit_status)(current);
    }

    pub(super) fn publish_status(&mut self) {
        let next = self.compute_status();
        let mut current = self.shared.status.lock().expect("index status lock");
        if *current == next {
            return;
        }
        let progress_only = FolderSizeIndexStatus {
            items_scanned: current.items_scanned,
            ..next.clone()
        } == *current;
        let now = (self.clock)();
        let emitted_recently = self.last_progress_emit.is_some_and(|last| {
            now.duration_since(last)
                .map_or(true, |age| age < PROGRESS_INTERVAL)
        });
        if progress_only && emitted_recently {
            return;
        }
        self.last_progress_emit = Some(now);
        *current = next.clone();
        drop(current);
        (self.emit_status)(next);
    }
}
