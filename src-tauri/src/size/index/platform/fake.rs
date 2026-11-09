use std::path::Path;
use std::sync::Mutex;

use super::{
    EventSink, IndexPlatform, PowerSignal, PowerSink, RawEvent, StreamHandle, StreamRequest,
};

pub struct FakePlatform {
    state: Mutex<State>,
}

struct State {
    event_id: u64,
    uuid: Option<String>,
    os_build: Option<String>,
    full_disk_access: bool,
    stream_fails: bool,
    conditions: PowerSignal,
    events: Option<EventSink>,
    power: Option<PowerSink>,
    stream_starts: Vec<Option<u64>>,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                event_id: 100,
                uuid: Some("FAKE-VOLUME".to_string()),
                os_build: Some("FAKE-BUILD".to_string()),
                full_disk_access: false,
                stream_fails: false,
                conditions: PowerSignal::Conditions {
                    low_power: false,
                    thermal_pressure: false,
                },
                events: None,
                power: None,
                stream_starts: Vec::new(),
            }),
        }
    }
}

impl FakePlatform {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("fake platform lock")
    }

    pub fn set_event_id(&self, id: u64) {
        self.state().event_id = id;
    }

    pub fn set_volume_uuid(&self, uuid: Option<&str>) {
        self.state().uuid = uuid.map(str::to_string);
    }

    pub fn set_os_build(&self, build: Option<&str>) {
        self.state().os_build = build.map(str::to_string);
    }

    pub fn set_full_disk_access(&self, granted: bool) {
        self.state().full_disk_access = granted;
    }

    pub fn fail_stream(&self) {
        self.state().stream_fails = true;
    }

    pub fn set_initial_conditions(&self, low_power: bool, thermal_pressure: bool) {
        self.state().conditions = PowerSignal::Conditions {
            low_power,
            thermal_pressure,
        };
    }

    pub fn emit(&self, events: Vec<RawEvent>) -> bool {
        let sink = self.state().events.clone();
        sink.map(|sink| sink(events)).is_some()
    }

    pub fn signal(&self, signal: PowerSignal) -> bool {
        let sink = self.state().power.clone();
        sink.map(|sink| sink(signal)).is_some()
    }

    pub fn stream_starts(&self) -> Vec<Option<u64>> {
        self.state().stream_starts.clone()
    }
}

impl IndexPlatform for FakePlatform {
    fn current_event_id(&self) -> u64 {
        self.state().event_id
    }

    fn volume_uuid(&self, _path: &Path) -> Option<String> {
        self.state().uuid.clone()
    }

    fn os_build(&self) -> Option<String> {
        self.state().os_build.clone()
    }

    fn start_stream(&self, request: StreamRequest<'_>, sink: EventSink) -> Option<StreamHandle> {
        let mut state = self.state();
        state.stream_starts.push(request.since);
        if state.stream_fails {
            return None;
        }
        state.events = Some(sink);
        Some(Box::new(()))
    }

    fn full_disk_access(&self) -> bool {
        self.state().full_disk_access
    }

    fn observe_power(&self, sink: PowerSink) -> PowerSignal {
        let mut state = self.state();
        state.power = Some(sink);
        state.conditions
    }
}
