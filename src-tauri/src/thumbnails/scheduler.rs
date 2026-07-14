//! Bounded thumbnail scheduler. Work is shared by cache fingerprint while
//! subscriber identity remains contextual and independently cancellable.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::ipc::types::ThumbnailPriority;
use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

use super::provider::ThumbnailProvider;
use super::types::{ThumbnailCacheKey, ThumbnailCandidate, ThumbnailState};

pub const MAX_ACTIVE_JOBS: usize = 2;
pub const MAX_DESIRED_PER_SCOPE: usize = 4_096;

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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ScopeKey {
    pane_id: String,
    tab_id: String,
    path: String,
    generation: u64,
}

impl From<&ThumbnailSubscriber> for ScopeKey {
    fn from(subscriber: &ThumbnailSubscriber) -> Self {
        Self {
            pane_id: subscriber.pane_id.clone(),
            tab_id: subscriber.tab_id.clone(),
            path: subscriber.path.clone(),
            generation: subscriber.generation,
        }
    }
}

#[derive(Clone)]
struct JobSubscriber {
    context: ThumbnailSubscriber,
    priority: ThumbnailPriority,
    order: u32,
}

struct Job {
    candidate: ThumbnailCandidate,
    subscribers: Vec<JobSubscriber>,
    active: bool,
}

struct State {
    queued: VecDeque<ThumbnailCacheKey>,
    jobs: HashMap<ThumbnailCacheKey, Job>,
    scope_revisions: HashMap<ScopeKey, u64>,
    active: usize,
    last_pane: Option<String>,
    stopped: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    provider: Arc<dyn ThumbnailProvider>,
    coordinator: Arc<ResourceCoordinator>,
    complete:
        Arc<dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync>,
    progress:
        Arc<dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync>,
    metadata_matches: Arc<dyn Fn(&ThumbnailCandidate) -> bool + Send + Sync>,
}

#[derive(Clone)]
pub struct ThumbnailScheduler {
    shared: Arc<Shared>,
    workers: Arc<Mutex<Option<Vec<JoinHandle<()>>>>>,
}

impl ThumbnailScheduler {
    pub fn new(
        provider: Arc<dyn ThumbnailProvider>,
        coordinator: Arc<ResourceCoordinator>,
        complete: Arc<
            dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync,
        >,
    ) -> Self {
        Self::new_with_progress_and_metadata_verifier(
            provider,
            coordinator,
            complete,
            Arc::new(|_, _, _| {}),
            Arc::new(|_| true),
        )
    }

    pub fn new_with_metadata_verifier(
        provider: Arc<dyn ThumbnailProvider>,
        coordinator: Arc<ResourceCoordinator>,
        complete: Arc<
            dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync,
        >,
        metadata_matches: Arc<dyn Fn(&ThumbnailCandidate) -> bool + Send + Sync>,
    ) -> Self {
        Self::new_with_progress_and_metadata_verifier(
            provider,
            coordinator,
            complete,
            Arc::new(|_, _, _| {}),
            metadata_matches,
        )
    }

    pub fn new_with_progress_and_metadata_verifier(
        provider: Arc<dyn ThumbnailProvider>,
        coordinator: Arc<ResourceCoordinator>,
        complete: Arc<
            dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync,
        >,
        progress: Arc<
            dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync,
        >,
        metadata_matches: Arc<dyn Fn(&ThumbnailCandidate) -> bool + Send + Sync>,
    ) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queued: VecDeque::new(),
                jobs: HashMap::new(),
                scope_revisions: HashMap::new(),
                active: 0,
                last_pane: None,
                stopped: false,
            }),
            wake: Condvar::new(),
            provider,
            coordinator,
            complete,
            progress,
            metadata_matches,
        });
        let workers = (0..MAX_ACTIVE_JOBS)
            .map(|index| {
                let shared = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("thumbnail-scheduler-{index}"))
                    .spawn(move || worker_loop(shared))
                    .expect("thumbnail scheduler worker")
            })
            .collect();
        Self {
            shared,
            workers: Arc::new(Mutex::new(Some(workers))),
        }
    }

    /// Replaces one scope's pending set without cancelling overlapping work.
    /// Older revisions are acknowledged as superseded and cannot overwrite a
    /// newer scroll range.
    pub fn replace_scope(
        &self,
        subscriber: ThumbnailSubscriber,
        revision: u64,
        candidates: Vec<(ThumbnailCandidate, ThumbnailPriority, u32)>,
    ) -> usize {
        if candidates.len() > MAX_DESIRED_PER_SCOPE {
            return 0;
        }
        let scope = ScopeKey::from(&subscriber);
        let desired = candidates
            .iter()
            .map(|(candidate, priority, order)| {
                (
                    ThumbnailCacheKey(candidate.fingerprint.clone()),
                    (*priority, *order),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut provider_cancels = Vec::new();
        let accepted = {
            let mut state = self.shared.state.lock().expect("thumbnail scheduler lock");
            if state.stopped {
                return 0;
            }
            if let Some(current) = state.scope_revisions.get(&scope) {
                if revision < *current {
                    return 0;
                }
                if revision == *current {
                    return candidates.len();
                }
            }
            state.scope_revisions.insert(scope, revision);

            let mut remove_queued = HashSet::new();
            for (key, job) in &mut state.jobs {
                let priority = desired.get(key).copied();
                job.subscribers.retain_mut(|existing| {
                    if !existing.context.matches(
                        &subscriber.pane_id,
                        &subscriber.tab_id,
                        &subscriber.path,
                        subscriber.generation,
                    ) {
                        return true;
                    }
                    if let Some((next_priority, next_order)) = priority {
                        existing.priority = next_priority;
                        existing.order = next_order;
                        true
                    } else {
                        false
                    }
                });
                if job.subscribers.is_empty() {
                    if job.active {
                        provider_cancels.push(job.candidate.clone());
                    } else {
                        remove_queued.insert(key.clone());
                    }
                }
            }
            for key in &remove_queued {
                state.jobs.remove(key);
            }
            state.queued.retain(|key| !remove_queued.contains(key));

            for (candidate, priority, order) in candidates {
                let key = ThumbnailCacheKey(candidate.fingerprint.clone());
                if let Some(job) = state.jobs.get_mut(&key) {
                    if !job.subscribers.iter().any(|existing| {
                        existing.context.matches(
                            &subscriber.pane_id,
                            &subscriber.tab_id,
                            &subscriber.path,
                            subscriber.generation,
                        )
                    }) {
                        job.subscribers.push(JobSubscriber {
                            context: subscriber.clone(),
                            priority,
                            order,
                        });
                    }
                } else {
                    state.jobs.insert(
                        key.clone(),
                        Job {
                            candidate,
                            subscribers: vec![JobSubscriber {
                                context: subscriber.clone(),
                                priority,
                                order,
                            }],
                            active: false,
                        },
                    );
                    state.queued.push_back(key);
                }
            }
            desired.len()
        };
        for candidate in provider_cancels {
            self.shared.provider.cancel(&candidate);
        }
        self.shared.wake.notify_all();
        accepted
    }

    /// Compatibility helper for direct scheduler callers and focused tests.
    pub fn submit(&self, subscriber: ThumbnailSubscriber, candidate: ThumbnailCandidate) {
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        {
            let mut state = self.shared.state.lock().expect("thumbnail scheduler lock");
            if state.stopped {
                return;
            }
            if let Some(job) = state.jobs.get_mut(&key) {
                job.subscribers.push(JobSubscriber {
                    context: subscriber,
                    priority: ThumbnailPriority::Visible,
                    order: 0,
                });
                return;
            }
            state.jobs.insert(
                key.clone(),
                Job {
                    candidate,
                    subscribers: vec![JobSubscriber {
                        context: subscriber,
                        priority: ThumbnailPriority::Visible,
                        order: 0,
                    }],
                    active: false,
                },
            );
            state.queued.push_back(key);
        }
        self.shared.wake.notify_one();
    }

    pub fn cancel_scope(&self, pane_id: &str, tab_id: &str, path: &str, generation: u64) {
        let mut provider_cancels = Vec::new();
        {
            let mut state = self.shared.state.lock().expect("thumbnail scheduler lock");
            state.scope_revisions.retain(|scope, _| {
                !(scope.pane_id == pane_id
                    && scope.tab_id == tab_id
                    && scope.path == path
                    && scope.generation == generation)
            });
            let mut remove_queued = HashSet::new();
            for (key, job) in &mut state.jobs {
                job.subscribers.retain(|subscriber| {
                    !subscriber
                        .context
                        .matches(pane_id, tab_id, path, generation)
                });
                if job.subscribers.is_empty() {
                    if job.active {
                        provider_cancels.push(job.candidate.clone());
                    } else {
                        remove_queued.insert(key.clone());
                    }
                }
            }
            for key in &remove_queued {
                state.jobs.remove(key);
            }
            state.queued.retain(|key| !remove_queued.contains(key));
        }
        for candidate in provider_cancels {
            self.shared.provider.cancel(&candidate);
        }
        self.shared.wake.notify_all();
    }

    pub fn shutdown(&self) {
        let candidates = {
            let mut state = self.shared.state.lock().expect("thumbnail scheduler lock");
            state.stopped = true;
            state.queued.clear();
            state.scope_revisions.clear();
            state
                .jobs
                .drain()
                .map(|(_, job)| job.candidate)
                .collect::<Vec<_>>()
        };
        for candidate in candidates {
            self.shared.provider.cancel(&candidate);
        }
        self.shared.provider.shutdown();
        self.shared.wake.notify_all();
        if let Some(workers) = self
            .workers
            .lock()
            .expect("thumbnail scheduler workers lock")
            .take()
        {
            for worker in workers {
                let _ = worker.join();
            }
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn counts(&self) -> (usize, usize) {
        let state = self.shared.state.lock().expect("thumbnail scheduler lock");
        (state.active, state.queued.len())
    }
}

impl Drop for ThumbnailScheduler {
    fn drop(&mut self) {
        if Arc::strong_count(&self.workers) == 1 {
            self.shutdown();
        }
    }
}

fn worker_loop(shared: Arc<Shared>) {
    loop {
        let (key, candidate) = {
            let mut state = shared.state.lock().expect("thumbnail scheduler lock");
            loop {
                if state.stopped {
                    return;
                }
                if let Some((key, candidate)) = take_next_job(&mut state) {
                    break (key, candidate);
                }
                state = shared.wake.wait(state).expect("thumbnail scheduler wake");
            }
        };

        let result = if !(shared.metadata_matches)(&candidate) {
            None
        } else {
            let permit = shared.coordinator.submit(JobSpec::new(
                [JobClass::Cpu],
                [format!(
                    "thumbnail:{}",
                    candidate.fingerprint.path.display()
                )],
            ));
            let result = if permit.is_ok() {
                let progress_shared = Arc::clone(&shared);
                let progress_key = key.clone();
                let progress_candidate = candidate.clone();
                shared.provider.generate(
                    &candidate,
                    Arc::new(move |preview| {
                        let subscribers = progress_shared
                            .state
                            .lock()
                            .expect("thumbnail scheduler lock")
                            .jobs
                            .get(&progress_key)
                            .map_or_else(Vec::new, |job| {
                                job.subscribers
                                    .iter()
                                    .map(|subscriber| subscriber.context.clone())
                                    .collect()
                            });
                        if !subscribers.is_empty() {
                            (progress_shared.progress)(
                                progress_candidate.clone(),
                                preview,
                                subscribers,
                            );
                        }
                    }),
                )
            } else {
                ThumbnailState::Failed
            };
            drop(permit);
            Some(result)
        };
        let subscribers = {
            let mut state = shared.state.lock().expect("thumbnail scheduler lock");
            state.active = state.active.saturating_sub(1);
            state.jobs.remove(&key).map_or_else(Vec::new, |job| {
                job.subscribers
                    .into_iter()
                    .map(|subscriber| subscriber.context)
                    .collect()
            })
        };
        if let (Some(result), false) = (result, subscribers.is_empty()) {
            (shared.complete)(candidate, result, subscribers);
        }
        shared.wake.notify_all();
    }
}

fn take_next_job(state: &mut State) -> Option<(ThumbnailCacheKey, ThumbnailCandidate)> {
    let last_pane = state.last_pane.as_deref();
    let (index, _, _, _, pane_id) = state
        .queued
        .iter()
        .enumerate()
        .filter_map(|(index, key)| {
            state.jobs.get(key).and_then(|job| {
                job.subscribers
                    .iter()
                    .min_by_key(|subscriber| (priority_rank(subscriber.priority), subscriber.order))
                    .map(|subscriber| {
                        (
                            index,
                            priority_rank(subscriber.priority),
                            usize::from(last_pane == Some(subscriber.context.pane_id.as_str())),
                            subscriber.order,
                            subscriber.context.pane_id.clone(),
                        )
                    })
            })
        })
        .min_by_key(|(_, priority, same_pane, order, _)| (*priority, *same_pane, *order))?;
    let key = state.queued.remove(index)?;
    let job = state.jobs.get_mut(&key)?;
    job.active = true;
    let candidate = job.candidate.clone();
    state.active += 1;
    state.last_pane = Some(pane_id);
    Some((key, candidate))
}

fn priority_rank(priority: ThumbnailPriority) -> u8 {
    match priority {
        ThumbnailPriority::Visible => 0,
        ThumbnailPriority::Ahead => 1,
        ThumbnailPriority::Behind => 2,
    }
}
