use std::time::{Duration, Instant};

use crate::ipc::types::IconStateEvent;

pub(crate) const MAX_BATCH: usize = 64;

pub(crate) const FLUSH_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) struct IconBatcher {
    buffer: Vec<IconStateEvent>,
    last_flush: Instant,
}

impl IconBatcher {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            buffer: Vec::new(),
            last_flush: now,
        }
    }

    pub(crate) fn push(
        &mut self,
        event: IconStateEvent,
        now: Instant,
    ) -> Option<Vec<IconStateEvent>> {
        self.buffer.push(event);
        let is_due =
            self.buffer.len() >= MAX_BATCH || now.duration_since(self.last_flush) >= FLUSH_INTERVAL;
        is_due.then(|| self.flush(now))
    }

    fn flush(&mut self, now: Instant) -> Vec<IconStateEvent> {
        self.last_flush = now;
        std::mem::take(&mut self.buffer)
    }

    pub(crate) fn drain_remainder(&mut self) -> Option<Vec<IconStateEvent>> {
        let has_remainder = !self.buffer.is_empty();
        has_remainder.then(|| std::mem::take(&mut self.buffer))
    }
}
