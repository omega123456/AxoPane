//! Bounded thumbnail scheduler. Work is shared by cache fingerprint while
//! subscriber identity remains contextual and independently cancellable.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

use super::provider::ThumbnailProvider;
use super::types::{ThumbnailCacheKey, ThumbnailCandidate, ThumbnailState};

pub const MAX_ACTIVE_JOBS: usize = 2;
pub const MAX_QUEUED_CANDIDATES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailSubscriber {
    pub pane_id: String,
    pub tab_id: String,
    pub path: String,
    pub generation: u64,
}

impl ThumbnailSubscriber {
    fn matches(&self, pane_id: &str, tab_id: &str, path: &str, generation: u64) -> bool {
        self.pane_id == pane_id
            && self.tab_id == tab_id
            && self.path == path
            && self.generation == generation
    }
}

struct Job {
    candidate: ThumbnailCandidate,
    subscribers: Vec<ThumbnailSubscriber>,
}
struct State {
    queued: VecDeque<ThumbnailCacheKey>,
    jobs: HashMap<ThumbnailCacheKey, Job>,
    active: usize,
    stopped: bool,
}

#[derive(Clone)]
pub struct ThumbnailScheduler {
    state: Arc<Mutex<State>>,
    provider: Arc<dyn ThumbnailProvider>,
    coordinator: Arc<ResourceCoordinator>,
    complete:
        Arc<dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync>,
    metadata_matches: Arc<dyn Fn(&ThumbnailCandidate) -> bool + Send + Sync>,
}

impl ThumbnailScheduler {
    pub fn new(
        provider: Arc<dyn ThumbnailProvider>,
        coordinator: Arc<ResourceCoordinator>,
        complete: Arc<
            dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync,
        >,
    ) -> Self {
        Self::new_with_metadata_verifier(provider, coordinator, complete, Arc::new(|_| true))
    }

    pub fn new_with_metadata_verifier(
        provider: Arc<dyn ThumbnailProvider>,
        coordinator: Arc<ResourceCoordinator>,
        complete: Arc<
            dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync,
        >,
        metadata_matches: Arc<dyn Fn(&ThumbnailCandidate) -> bool + Send + Sync>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                queued: VecDeque::new(),
                jobs: HashMap::new(),
                active: 0,
                stopped: false,
            })),
            provider,
            coordinator,
            complete,
            metadata_matches,
        }
    }
    pub fn submit(&self, subscriber: ThumbnailSubscriber, candidate: ThumbnailCandidate) {
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        {
            let mut state = self.state.lock().expect("thumbnail scheduler lock");
            if state.stopped {
                return;
            }
            if let Some(job) = state.jobs.get_mut(&key) {
                job.subscribers.push(subscriber);
                return;
            }
            if state.queued.len() >= MAX_QUEUED_CANDIDATES {
                return;
            }
            state.jobs.insert(
                key.clone(),
                Job {
                    candidate,
                    subscribers: vec![subscriber],
                },
            );
            state.queued.push_back(key);
        }
        self.pump();
    }
    pub fn cancel_scope(&self, pane_id: &str, tab_id: &str, path: &str, generation: u64) {
        let mut provider_cancels = Vec::new();
        {
            let mut state = self.state.lock().expect("thumbnail scheduler lock");
            let keys = state
                .jobs
                .iter_mut()
                .filter_map(|(key, job)| {
                    job.subscribers
                        .retain(|s| !s.matches(pane_id, tab_id, path, generation));
                    job.subscribers.is_empty().then_some(key.clone())
                })
                .collect::<Vec<_>>();
            for key in keys {
                if let Some(job) = state.jobs.remove(&key) {
                    state.queued.retain(|queued| queued != &key);
                    provider_cancels.push(job.candidate);
                }
            }
        }
        for candidate in provider_cancels {
            self.provider.cancel(&candidate);
        }
    }
    pub fn shutdown(&self) {
        let candidates = {
            let mut s = self.state.lock().expect("thumbnail scheduler lock");
            s.stopped = true;
            s.queued.clear();
            s.jobs.drain().map(|(_, j)| j.candidate).collect::<Vec<_>>()
        };
        for c in candidates {
            self.provider.cancel(&c);
        }
        self.provider.shutdown();
    }
    #[cfg(feature = "test-utils")]
    pub fn counts(&self) -> (usize, usize) {
        let s = self.state.lock().expect("thumbnail scheduler lock");
        (s.active, s.queued.len())
    }
    fn pump(&self) {
        loop {
            let key = {
                let mut s = self.state.lock().expect("thumbnail scheduler lock");
                if s.stopped || s.active >= MAX_ACTIVE_JOBS {
                    return;
                }
                let Some(key) = s.queued.pop_front() else {
                    return;
                };
                if !s.jobs.contains_key(&key) {
                    continue;
                }
                s.active += 1;
                key
            };
            let scheduler = self.clone();
            let state = Arc::clone(&self.state);
            let provider = Arc::clone(&self.provider);
            let coordinator = Arc::clone(&self.coordinator);
            let complete = Arc::clone(&self.complete);
            let metadata_matches = Arc::clone(&self.metadata_matches);
            std::thread::spawn(move || {
                let candidate = {
                    let s = state.lock().expect("thumbnail scheduler lock");
                    s.jobs.get(&key).map(|job| job.candidate.clone())
                };
                if let Some(candidate) = candidate {
                    let result = if !(metadata_matches)(&candidate) {
                        None
                    } else {
                        let permit = coordinator.submit(JobSpec::new(
                            [JobClass::Cpu],
                            [format!(
                                "thumbnail:{}",
                                candidate.fingerprint.path.display()
                            )],
                        ));
                        let result = if permit.is_ok() {
                            provider.generate(&candidate)
                        } else {
                            ThumbnailState::Failed
                        };
                        drop(permit);
                        (metadata_matches)(&candidate).then_some(result)
                    };
                    let subscribers = {
                        let mut s = state.lock().expect("thumbnail scheduler lock");
                        s.active = s.active.saturating_sub(1);
                        s.jobs
                            .remove(&key)
                            .map_or_else(Vec::new, |job| job.subscribers)
                    };
                    if let (Some(result), false) = (result, subscribers.is_empty()) {
                        complete(candidate, result, subscribers);
                    }
                } else {
                    let mut s = state.lock().expect("thumbnail scheduler lock");
                    s.active = s.active.saturating_sub(1);
                }
                scheduler.pump();
            });
        }
    }
}
