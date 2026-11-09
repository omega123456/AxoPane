use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

pub const MAX_SIZE_METADATA_ENTRIES: usize = 10_000;

pub struct ScheduledJob<T> {
    pub path: String,
    pub cancelled: Arc<AtomicBool>,
    pub payload: T,
}

struct JobEntry<T> {
    cancelled: Arc<AtomicBool>,
    payload: Option<T>,
}

struct State<T> {
    queued: VecDeque<String>,
    jobs: HashMap<String, JobEntry<T>>,
    shutdown: bool,
}

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

    pub fn schedule(&self, job: ScheduledJob<T>) -> bool {
        let ScheduledJob {
            path,
            cancelled,
            payload,
        } = job;
        let mut state = self.state.lock().expect("size scheduler lock");
        let already_scheduled = state.jobs.contains_key(&path);
        let at_capacity = state.jobs.len() >= self.capacity;
        if already_scheduled || at_capacity {
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
        state.queued.retain(|queued| queued != path);
        true
    }

    pub fn cancel_many(&self, paths: &[String]) -> usize {
        paths.iter().filter(|path| self.cancel(path)).count()
    }

    pub fn next(&self) -> Option<ScheduledJob<T>> {
        let mut state = self.state.lock().expect("size scheduler lock");
        let mut claimable = pop_claimable(&mut state);
        while must_wait_for_job(&claimable, &state) {
            state = self.ready.wait(state).expect("size scheduler condvar");
            claimable = pop_claimable(&mut state);
        }
        claimable
    }

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

fn must_wait_for_job<T>(claimable: &Option<ScheduledJob<T>>, state: &State<T>) -> bool {
    claimable.is_none() && !state.shutdown
}

fn pop_claimable<T>(state: &mut State<T>) -> Option<ScheduledJob<T>> {
    while let Some(path) = state.queued.pop_front() {
        if let Some(job) = take_payload(&mut state.jobs, &path) {
            return Some(job);
        }
    }
    None
}

fn take_payload<T>(jobs: &mut HashMap<String, JobEntry<T>>, path: &str) -> Option<ScheduledJob<T>> {
    let entry = jobs.get_mut(path)?;
    let payload = entry.payload.take()?;
    Some(ScheduledJob {
        path: path.to_string(),
        cancelled: Arc::clone(&entry.cancelled),
        payload,
    })
}
