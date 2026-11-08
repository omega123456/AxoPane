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

    pub(super) fn compute_status(&self) -> FolderSizeIndexStatus {
        if self.stream.is_none() && !matches!(self.phase, Phase::Booting) {
            return FolderSizeIndexStatus::new(
                IndexBackend::MacIndex,
                false,
                IndexState::Unavailable,
            );
        }
        let mut status =
            FolderSizeIndexStatus::new(IndexBackend::MacIndex, true, IndexState::NotReady);
        let background = self.full.is_some() || self.background_inflight > 0;
        let pause_reason = if self.low_power {
            Some(PauseReason::LowPowerMode)
        } else if self.thermal_pressure {
            Some(PauseReason::Thermal)
        } else {
            None
        };
        status.state = match (&self.phase, pause_reason) {
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
            _ if background => IndexState::Updating,
            _ => IndexState::UpToDate,
        };
        status
    }

    /// Emits on every transition; item-count progress at most once per
    /// [`PROGRESS_INTERVAL`] of the injected clock.
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
        if progress_only
            && self.last_progress_emit.is_some_and(|last| {
                now.duration_since(last)
                    .map_or(true, |age| age < PROGRESS_INTERVAL)
            })
        {
            return;
        }
        self.last_progress_emit = Some(now);
        *current = next.clone();
        drop(current);
        (self.emit_status)(next);
    }
}
