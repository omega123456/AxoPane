//! Bounded thumbnail scheduler. Work is shared by cache fingerprint while
//! subscriber identity remains contextual and independently cancellable.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::ipc::types::ThumbnailPriority;
use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

use super::provider::{ThumbnailPreviewCallback, ThumbnailProvider};
use super::types::{ThumbnailCacheKey, ThumbnailCandidate, ThumbnailState};

pub type ResultPublisher =
    Arc<dyn Fn(ThumbnailCandidate, ThumbnailState, Vec<ThumbnailSubscriber>) + Send + Sync>;
pub type MetadataMatcher = Arc<dyn Fn(&ThumbnailCandidate) -> bool + Send + Sync>;

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

impl JobSubscriber {
    fn visible(context: ThumbnailSubscriber) -> Self {
        Self {
            context,
            priority: ThumbnailPriority::Visible,
            order: 0,
        }
    }

    fn rank_key(&self) -> (u8, u32) {
        (priority_rank(self.priority), self.order)
    }

    fn keep_after_retarget(
        &mut self,
        subscriber: &ThumbnailSubscriber,
        rank: Option<SubscriberRank>,
    ) -> bool {
        if !self.context.same_scope(subscriber) {
            return true;
        }
        let Some(rank) = rank else {
            return false;
        };
        self.priority = rank.priority;
        self.order = rank.order;
        true
    }
}

struct Job {
    candidate: ThumbnailCandidate,
    subscribers: Vec<JobSubscriber>,
    active: bool,
}

#[derive(Clone, Copy)]
struct SubscriberRank {
    priority: ThumbnailPriority,
    order: u32,
}

enum ScopeReplacement {
    Ignored {
        accepted: usize,
    },
    Applied {
        accepted: usize,
        provider_cancels: Vec<ThumbnailCandidate>,
    },
}

impl ThumbnailSubscriber {
    fn same_scope(&self, other: &ThumbnailSubscriber) -> bool {
        self.matches(&other.pane_id, &other.tab_id, &other.path, other.generation)
    }
}

impl Job {
    fn has_subscriber(&self, subscriber: &ThumbnailSubscriber) -> bool {
        self.subscribers
            .iter()
            .any(|existing| existing.context.same_scope(subscriber))
    }

    fn retarget_subscriber(
        &mut self,
        subscriber: &ThumbnailSubscriber,
        rank: Option<SubscriberRank>,
    ) {
        self.subscribers
            .retain_mut(|existing| existing.keep_after_retarget(subscriber, rank));
    }

    fn drop_subscribers_matching(
        &mut self,
        pane_id: &str,
        tab_id: &str,
        path: &str,
        generation: u64,
    ) {
        self.subscribers.retain(|subscriber| {
            !subscriber
                .context
                .matches(pane_id, tab_id, path, generation)
        });
    }

    fn add_subscriber_once(&mut self, job_subscriber: JobSubscriber) {
        if self.has_subscriber(&job_subscriber.context) {
            return;
        }
        self.subscribers.push(job_subscriber);
    }

    fn best_subscriber(&self) -> Option<&JobSubscriber> {
        self.subscribers
            .iter()
            .min_by_key(|subscriber| subscriber.rank_key())
    }
}

#[derive(Default)]
struct UnsubscribedJobs {
    provider_cancels: Vec<ThumbnailCandidate>,
    queued: HashSet<ThumbnailCacheKey>,
}

impl UnsubscribedJobs {
    fn record(&mut self, key: &ThumbnailCacheKey, job: &Job) {
        if !job.subscribers.is_empty() {
            return;
        }
        if job.active {
            self.provider_cancels.push(job.candidate.clone());
            return;
        }
        self.queued.insert(key.clone());
    }
}

struct QueuedChoice {
    index: usize,
    priority: u8,
    same_pane: usize,
    order: u32,
    pane_id: String,
}

impl QueuedChoice {
    fn rank_key(&self) -> (u8, usize, u32) {
        (self.priority, self.same_pane, self.order)
    }
}

fn desired_ranks(
    candidates: &[(ThumbnailCandidate, ThumbnailPriority, u32)],
) -> HashMap<ThumbnailCacheKey, SubscriberRank> {
    candidates
        .iter()
        .map(|(candidate, priority, order)| {
            let rank = SubscriberRank {
                priority: *priority,
                order: *order,
            };
            (ThumbnailCacheKey(candidate.fingerprint.clone()), rank)
        })
        .collect()
}

struct State {
    queued: VecDeque<ThumbnailCacheKey>,
    jobs: HashMap<ThumbnailCacheKey, Job>,
    scope_revisions: HashMap<ScopeKey, u64>,
    active: usize,
    last_pane: Option<String>,
    stopped: bool,
}

impl State {
    fn replace_scope(
        &mut self,
        subscriber: &ThumbnailSubscriber,
        revision: u64,
        candidates: Vec<(ThumbnailCandidate, ThumbnailPriority, u32)>,
    ) -> ScopeReplacement {
        if self.stopped {
            return ScopeReplacement::Ignored { accepted: 0 };
        }
        let scope = ScopeKey::from(subscriber);
        if let Some(accepted) = self.ignored_revision(&scope, revision, candidates.len()) {
            return ScopeReplacement::Ignored { accepted };
        }
        self.scope_revisions.insert(scope, revision);

        let desired = desired_ranks(&candidates);
        let provider_cancels = self.retarget_scope_jobs(subscriber, &desired);
        self.subscribe_candidates(subscriber, candidates);
        ScopeReplacement::Applied {
            accepted: desired.len(),
            provider_cancels,
        }
    }

    fn ignored_revision(
        &self,
        scope: &ScopeKey,
        revision: u64,
        candidate_count: usize,
    ) -> Option<usize> {
        let current = *self.scope_revisions.get(scope)?;
        if revision < current {
            return Some(0);
        }
        if revision == current {
            return Some(candidate_count);
        }
        None
    }

    fn retarget_scope_jobs(
        &mut self,
        subscriber: &ThumbnailSubscriber,
        desired: &HashMap<ThumbnailCacheKey, SubscriberRank>,
    ) -> Vec<ThumbnailCandidate> {
        self.drop_unsubscribed_jobs(|key, job| {
            job.retarget_subscriber(subscriber, desired.get(key).copied());
        })
    }

    fn drop_unsubscribed_jobs(
        &mut self,
        mut update_job: impl FnMut(&ThumbnailCacheKey, &mut Job),
    ) -> Vec<ThumbnailCandidate> {
        let mut unsubscribed = UnsubscribedJobs::default();
        for (key, job) in &mut self.jobs {
            update_job(key, job);
            unsubscribed.record(key, job);
        }
        for key in &unsubscribed.queued {
            self.jobs.remove(key);
        }
        self.queued.retain(|key| !unsubscribed.queued.contains(key));
        unsubscribed.provider_cancels
    }

    fn subscribe_candidates(
        &mut self,
        subscriber: &ThumbnailSubscriber,
        candidates: Vec<(ThumbnailCandidate, ThumbnailPriority, u32)>,
    ) {
        for (candidate, priority, order) in candidates {
            let job_subscriber = JobSubscriber {
                context: subscriber.clone(),
                priority,
                order,
            };
            self.add_subscriber(candidate, job_subscriber);
        }
    }

    fn add_subscriber(&mut self, candidate: ThumbnailCandidate, job_subscriber: JobSubscriber) {
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        if let Some(job) = self.jobs.get_mut(&key) {
            job.add_subscriber_once(job_subscriber);
            return;
        }
        self.queue_new_job(key, candidate, job_subscriber);
    }

    fn queue_new_job(
        &mut self,
        key: ThumbnailCacheKey,
        candidate: ThumbnailCandidate,
        job_subscriber: JobSubscriber,
    ) {
        self.jobs.insert(
            key.clone(),
            Job {
                candidate,
                subscribers: vec![job_subscriber],
                active: false,
            },
        );
        self.queued.push_back(key);
    }

    fn submit_visible(
        &mut self,
        subscriber: ThumbnailSubscriber,
        candidate: ThumbnailCandidate,
    ) -> bool {
        if self.stopped {
            return false;
        }
        let key = ThumbnailCacheKey(candidate.fingerprint.clone());
        let job_subscriber = JobSubscriber::visible(subscriber);
        if let Some(job) = self.jobs.get_mut(&key) {
            job.subscribers.push(job_subscriber);
            return false;
        }
        self.queue_new_job(key, candidate, job_subscriber);
        true
    }
}

impl State {
    /// Drops the scope's subscriptions and its unsubscribed queued jobs;
    /// returns the candidates of active jobs left without subscribers.
    fn remove_scope(
        &mut self,
        pane_id: &str,
        tab_id: &str,
        path: &str,
        generation: u64,
    ) -> Vec<ThumbnailCandidate> {
        self.scope_revisions.retain(|scope, _| {
            !(scope.pane_id == pane_id
                && scope.tab_id == tab_id
                && scope.path == path
                && scope.generation == generation)
        });
        self.drop_unsubscribed_jobs(|_, job| {
            job.drop_subscribers_matching(pane_id, tab_id, path, generation);
        })
    }

    fn finish_job(&mut self, key: &ThumbnailCacheKey) -> Vec<ThumbnailSubscriber> {
        self.active = self.active.saturating_sub(1);
        self.jobs.remove(key).map_or_else(Vec::new, |job| {
            job.subscribers
                .into_iter()
                .map(|subscriber| subscriber.context)
                .collect()
        })
    }

    fn subscriber_contexts(&self, key: &ThumbnailCacheKey) -> Vec<ThumbnailSubscriber> {
        self.jobs.get(key).map_or_else(Vec::new, |job| {
            job.subscribers
                .iter()
                .map(|subscriber| subscriber.context.clone())
                .collect()
        })
    }
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    provider: Arc<dyn ThumbnailProvider>,
    coordinator: Arc<ResourceCoordinator>,
    complete: ResultPublisher,
    progress: ResultPublisher,
    metadata_matches: MetadataMatcher,
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
        complete: ResultPublisher,
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
        complete: ResultPublisher,
        metadata_matches: MetadataMatcher,
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
        complete: ResultPublisher,
        progress: ResultPublisher,
        metadata_matches: MetadataMatcher,
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

    pub fn replace_scope(
        &self,
        subscriber: ThumbnailSubscriber,
        revision: u64,
        candidates: Vec<(ThumbnailCandidate, ThumbnailPriority, u32)>,
    ) -> usize {
        if candidates.len() > MAX_DESIRED_PER_SCOPE {
            return 0;
        }
        let replacement = self
            .shared
            .state
            .lock()
            .expect("thumbnail scheduler lock")
            .replace_scope(&subscriber, revision, candidates);
        let (accepted, provider_cancels) = match replacement {
            ScopeReplacement::Ignored { accepted } => return accepted,
            ScopeReplacement::Applied {
                accepted,
                provider_cancels,
            } => (accepted, provider_cancels),
        };
        self.cancel_in_provider(provider_cancels);
        self.shared.wake.notify_all();
        accepted
    }

    fn cancel_in_provider(&self, candidates: Vec<ThumbnailCandidate>) {
        for candidate in candidates {
            self.shared.provider.cancel(&candidate);
        }
    }

    /// Compatibility helper for direct scheduler callers and focused tests.
    pub fn submit(&self, subscriber: ThumbnailSubscriber, candidate: ThumbnailCandidate) {
        let queued_new_job = self
            .shared
            .state
            .lock()
            .expect("thumbnail scheduler lock")
            .submit_visible(subscriber, candidate);
        if queued_new_job {
            self.shared.wake.notify_one();
        }
    }

    pub fn cancel_scope(&self, pane_id: &str, tab_id: &str, path: &str, generation: u64) {
        let provider_cancels = self
            .shared
            .state
            .lock()
            .expect("thumbnail scheduler lock")
            .remove_scope(pane_id, tab_id, path, generation);
        self.cancel_in_provider(provider_cancels);
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
        self.cancel_in_provider(candidates);
        self.shared.provider.shutdown();
        self.shared.wake.notify_all();
        let workers = self
            .workers
            .lock()
            .expect("thumbnail scheduler workers lock")
            .take()
            .unwrap_or_default();
        for worker in workers {
            let _ = worker.join();
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
    while let Some((key, candidate)) = wait_for_next_job(&shared) {
        let result = generate_if_current(&shared, &key, &candidate);
        let subscribers = shared
            .state
            .lock()
            .expect("thumbnail scheduler lock")
            .finish_job(&key);
        if let (Some(result), false) = (result, subscribers.is_empty()) {
            (shared.complete)(candidate, result, subscribers);
        }
        shared.wake.notify_all();
    }
}

fn wait_for_next_job(shared: &Shared) -> Option<(ThumbnailCacheKey, ThumbnailCandidate)> {
    let mut state = shared.state.lock().expect("thumbnail scheduler lock");
    loop {
        if state.stopped {
            return None;
        }
        if let Some(next_job) = take_next_job(&mut state) {
            return Some(next_job);
        }
        state = shared.wake.wait(state).expect("thumbnail scheduler wake");
    }
}

/// `None` when the file changed since it was requested.
fn generate_if_current(
    shared: &Arc<Shared>,
    key: &ThumbnailCacheKey,
    candidate: &ThumbnailCandidate,
) -> Option<ThumbnailState> {
    if !(shared.metadata_matches)(candidate) {
        return None;
    }
    let permit = shared.coordinator.submit(JobSpec::new(
        [JobClass::Cpu],
        [format!(
            "thumbnail:{}",
            candidate.fingerprint.path.display()
        )],
    ));
    if permit.is_err() {
        return Some(ThumbnailState::Failed);
    }
    Some(
        shared
            .provider
            .generate(candidate, preview_reporter(shared, key, candidate)),
    )
}

/// Forwards each preview to the job's current subscribers, if any remain.
fn preview_reporter(
    shared: &Arc<Shared>,
    key: &ThumbnailCacheKey,
    candidate: &ThumbnailCandidate,
) -> ThumbnailPreviewCallback {
    let shared = Arc::clone(shared);
    let key = key.clone();
    let candidate = candidate.clone();
    Arc::new(move |preview| {
        let subscribers = shared
            .state
            .lock()
            .expect("thumbnail scheduler lock")
            .subscriber_contexts(&key);
        if !subscribers.is_empty() {
            (shared.progress)(candidate.clone(), preview, subscribers);
        }
    })
}

fn take_next_job(state: &mut State) -> Option<(ThumbnailCacheKey, ThumbnailCandidate)> {
    let last_pane = state.last_pane.as_deref();
    let choice = state
        .queued
        .iter()
        .enumerate()
        .filter_map(|(index, key)| queued_choice(state, index, key, last_pane))
        .min_by_key(QueuedChoice::rank_key)?;
    let key = state.queued.remove(choice.index)?;
    let job = state.jobs.get_mut(&key)?;
    job.active = true;
    let candidate = job.candidate.clone();
    state.active += 1;
    state.last_pane = Some(choice.pane_id);
    Some((key, candidate))
}

fn queued_choice(
    state: &State,
    index: usize,
    key: &ThumbnailCacheKey,
    last_pane: Option<&str>,
) -> Option<QueuedChoice> {
    let subscriber = state.jobs.get(key)?.best_subscriber()?;
    let pane_id = subscriber.context.pane_id.clone();
    Some(QueuedChoice {
        index,
        priority: priority_rank(subscriber.priority),
        same_pane: usize::from(last_pane == Some(pane_id.as_str())),
        order: subscriber.order,
        pane_id,
    })
}

fn priority_rank(priority: ThumbnailPriority) -> u8 {
    match priority {
        ThumbnailPriority::Visible => 0,
        ThumbnailPriority::Ahead => 1,
        ThumbnailPriority::Behind => 2,
    }
}
