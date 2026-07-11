//! Coalescing ownership for folder-size work.
//!
//! A path exists at most once in the queue.  Cancelling removes its record
//! before a worker can observe it, rather than leaving cancelled work to drain.
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

pub const MAX_SIZE_METADATA_ENTRIES: usize = 10_000;

pub struct ScheduledJob<T> {
    pub path: String,
    pub cancelled: Arc<AtomicBool>,
    pub payload: T,
}

/// Internal per-path bookkeeping. `payload` is taken (moved out) by whichever
/// of `next`/`claim` first observes this path, leaving `None` behind — the
/// entry itself is deliberately retained afterward (not removed) so
/// `schedule`'s `contains_key` coalescing check keeps rejecting duplicate
/// requests for a path that is still executing, right up until `complete`
/// removes it. `T` need not be `Clone`: a payload is claimed by move, exactly
/// once, matching a one-shot `FnOnce` work closure.
struct JobEntry<T> {
    cancelled: Arc<AtomicBool>,
    payload: Option<T>,
}

struct State<T> {
    queued: VecDeque<String>,
    jobs: HashMap<String, JobEntry<T>>,
    shutdown: bool,
}

/// A bounded, removable, path-coalescing queue.  The dedicated worker is
/// deliberately owned by the caller; this makes it possible to keep exactly
/// one Everything worker while admitting manual traversals separately.
pub struct SizeScheduler<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
    capacity: usize,
}

impl<T> SizeScheduler<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State {
                queued: VecDeque::new(),
                jobs: HashMap::new(),
                shutdown: false,
            }),
            ready: Condvar::new(),
            capacity: capacity.max(1),
        }
    }

    /// Returns false when this path was already pending; its existing work is
    /// retained, which is the coalescing contract.
    ///
    /// `cancelled` is the caller's own identity token for this job (e.g.
    /// `SizeService`'s per-path `Arc<AtomicBool>`, also the one stored in its
    /// own job registry and handed back to callers). It is stored verbatim
    /// as this entry's identity rather than the scheduler minting its own —
    /// that is what lets [`SizeScheduler::complete`]'s `Arc::ptr_eq` check
    /// actually authorize removal against the same token the caller tracks,
    /// instead of comparing against a disconnected scheduler-internal one
    /// that could never match.
    pub fn schedule(&self, path: String, cancelled: Arc<AtomicBool>, payload: T) -> bool {
        let mut state = self.state.lock().expect("size scheduler lock");
        if state.jobs.contains_key(&path) || state.jobs.len() >= self.capacity {
            return false;
        }
        state.jobs.insert(
            path.clone(),
            JobEntry {
                cancelled,
                payload: Some(payload),
            },
        );
        state.queued.push_back(path);
        self.ready.notify_one();
        true
    }

    pub fn cancel(&self, path: &str) -> bool {
        let mut state = self.state.lock().expect("size scheduler lock");
        let Some(_job) = state.jobs.remove(path) else {
            return false;
        };
        // `_job.cancelled` intentionally not flipped further here: it is
        // removed outright, matching a job that is no longer trackable by
        // anyone (the caller's own external cancel token, stored alongside
        // this scheduler entry by `SizeService`, is what a worker actually
        // observes — see `SizeService::cancel`). Removing rather than merely
        // flagging keeps `pending_len`/capacity accounting exact.
        state.queued.retain(|queued| queued != path);
        true
    }

    pub fn cancel_many(&self, paths: &[String]) -> usize {
        paths.iter().filter(|path| self.cancel(path)).count()
    }

    /// Blocks until a queued path's payload can be claimed, or the scheduler
    /// shuts down. Skips (and keeps draining past) any path whose payload
    /// was already claimed by a concurrent `next`/`claim` call, or whose
    /// entry was removed by `cancel`/`complete` before this call reached it.
    pub fn next(&self) -> Option<ScheduledJob<T>> {
        let mut state = self.state.lock().expect("size scheduler lock");
        loop {
            while let Some(path) = state.queued.pop_front() {
                if let Some(job) = take_payload(&mut state.jobs, &path) {
                    return Some(job);
                }
            }
            if state.shutdown {
                return None;
            }
            state = self.ready.wait(state).expect("size scheduler condvar");
        }
    }

    /// Claims one particular queued target's payload. This is used by a
    /// worker that was already assigned its wake-up by the service;
    /// cancellation has removed the target before this point, so it cannot
    /// execute stale work.
    pub fn claim(&self, path: &str) -> Option<ScheduledJob<T>> {
        let mut state = self.state.lock().expect("size scheduler lock");
        let job = take_payload(&mut state.jobs, path);
        state.queued.retain(|queued| queued != path);
        job
    }

    pub fn complete(&self, path: &str, cancelled: &Arc<AtomicBool>) {
        let mut state = self.state.lock().expect("size scheduler lock");
        if state
            .jobs
            .get(path)
            .is_some_and(|job| Arc::ptr_eq(&job.cancelled, cancelled))
        {
            state.jobs.remove(path);
        }
    }

    pub fn pending_len(&self) -> usize {
        self.state.lock().expect("size scheduler lock").jobs.len()
    }

    pub fn shutdown(&self) {
        let mut state = self.state.lock().expect("size scheduler lock");
        state.shutdown = true;
        for job in state.jobs.values() {
            job.cancelled.store(true, Ordering::Relaxed);
        }
        state.jobs.clear();
        state.queued.clear();
        self.ready.notify_all();
    }
}

/// Takes `path`'s payload out of `jobs` if present and not already claimed,
/// returning `None` (and leaving the entry's bookkeeping otherwise intact)
/// if it was already claimed by a concurrent caller or the entry is gone.
fn take_payload<T>(jobs: &mut HashMap<String, JobEntry<T>>, path: &str) -> Option<ScheduledJob<T>> {
    let entry = jobs.get_mut(path)?;
    let payload = entry.payload.take()?;
    Some(ScheduledJob {
        path: path.to_string(),
        cancelled: Arc::clone(&entry.cancelled),
        payload,
    })
}
