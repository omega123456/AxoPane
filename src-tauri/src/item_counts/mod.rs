use std::collections::{HashMap, VecDeque};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::fs::{
    self, DirectoryEntry, FsError, ListDirOptions, ListDirOutcome, SortDirection, SortKey,
};

pub mod cache;
use cache::{ItemCountCache, ItemCountState};

pub const AUTO_ITEM_COUNT_LIMIT: usize = 200;
pub const ITEM_COUNT_BATCH_SIZE: usize = 64;
pub const MAX_AUTOMATIC_ITEM_COUNT_WORKERS: usize = 2;
/// Bound queued viewport plans as well as workers: stale scroll/navigation
/// requests must not become an unbounded backlog while I/O is busy.
pub const MAX_AUTOMATIC_ITEM_COUNT_QUEUE: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemCountRequestContext {
    pub pane_id: String,
    pub tab_id: String,
    pub request_id: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VisibleItemCountsRequest {
    pub context: ItemCountRequestContext,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemCountResult {
    pub path: String,
    pub item_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemCountEvent {
    pub context: ItemCountRequestContext,
    pub results: Vec<ItemCountResult>,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveItemsSortRequest {
    pub context: ItemCountRequestContext,
    pub sort_direction: SortDirection,
    pub filter: String,
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveItemsSortReady {
    pub context: ItemCountRequestContext,
    pub path: String,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveItemsSortSuperseded {
    pub context: ItemCountRequestContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ActiveItemsSortResponse {
    Ready(ActiveItemsSortReady),
    Superseded(ActiveItemsSortSuperseded),
}

#[derive(Debug, Clone)]
pub struct AutomaticCountPlan {
    scope_key: String,
    token: u64,
    context: ItemCountRequestContext,
    /// The active parent session's watch revision when available.  Counts are
    /// keyed by the generation that observed their parent directory, so
    /// viewport counting and an Items sort of that same session share entries.
    generation: u64,
    paths: Vec<String>,
}

impl AutomaticCountPlan {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

#[derive(Debug, Clone)]
struct ExplicitCountHandle {
    scope_key: String,
    token: u64,
    context: ItemCountRequestContext,
}

#[derive(Debug, Default)]
struct AutomaticScopeState {
    next_token: u64,
    active_token: Option<u64>,
    active_context: Option<ItemCountRequestContext>,
    requested_paths: Vec<String>,
}

#[derive(Debug, Default)]
struct ExplicitScopeState {
    next_token: u64,
    active_token: Option<u64>,
    active_context: Option<ItemCountRequestContext>,
}

#[derive(Debug, Default, Clone)]
pub struct ItemCountService {
    cache: Arc<Mutex<ItemCountCache>>,
    automatic: Arc<Mutex<HashMap<String, AutomaticScopeState>>>,
    explicit: Arc<Mutex<HashMap<String, ExplicitScopeState>>>,
    automatic_queue: Arc<Mutex<VecDeque<AutomaticCountPlan>>>,
    automatic_workers: Arc<Mutex<usize>>,
    #[cfg(feature = "test-utils")]
    test_events: Arc<Mutex<Vec<ItemCountEvent>>>,
}

impl ItemCountService {
    pub fn cancel_tab(&self, tab_id: &str) {
        self.automatic
            .lock()
            .expect("item-count automatic lock")
            .retain(|_, state| {
                state
                    .active_context
                    .as_ref()
                    .is_none_or(|context| context.tab_id != tab_id)
            });
        self.explicit
            .lock()
            .expect("item-count explicit lock")
            .retain(|_, state| {
                state
                    .active_context
                    .as_ref()
                    .is_none_or(|context| context.tab_id != tab_id)
            });
        self.automatic_queue
            .lock()
            .expect("item-count queue lock")
            .retain(|plan| plan.context.tab_id != tab_id);
    }

    /// Queues bounded automatic work. Returns true only when a worker should
    /// be started; callers therefore never create an unbounded thread per IPC.
    pub fn enqueue_automatic_request(&self, plan: AutomaticCountPlan) -> bool {
        if plan.is_empty() {
            return false;
        }
        let mut queue = self.automatic_queue.lock().expect("item-count queue lock");
        // Only the newest viewport plan for a pane/tab can still be useful.
        // `plan_automatic_request` carries its outstanding paths forward, so
        // replacing a queued plan preserves required visible-count progress.
        queue.retain(|queued| queued.scope_key != plan.scope_key);
        while queue.len() >= MAX_AUTOMATIC_ITEM_COUNT_QUEUE {
            queue.pop_front();
        }
        queue.push_back(plan);
        drop(queue);
        self.try_claim_automatic_worker()
    }

    fn try_claim_automatic_worker(&self) -> bool {
        let mut workers = self
            .automatic_workers
            .lock()
            .expect("item-count workers lock");
        if *workers >= MAX_AUTOMATIC_ITEM_COUNT_WORKERS {
            return false;
        }
        *workers += 1;
        true
    }

    /// An enqueue can race the empty pop while all workers are winding down.
    /// Reclaims this thread when work appeared; otherwise a later enqueue
    /// observes the released slot and starts a bounded replacement worker.
    fn reclaim_worker_for_raced_work(&self) -> bool {
        *self
            .automatic_workers
            .lock()
            .expect("item-count workers lock") -= 1;
        let queue_empty = self
            .automatic_queue
            .lock()
            .expect("item-count queue lock")
            .is_empty();
        if queue_empty {
            return false;
        }
        self.try_claim_automatic_worker()
    }

    pub fn process_automatic_queue<F>(&self, mut emit: F)
    where
        F: FnMut(ItemCountEvent),
    {
        self.drain_automatic_queue(&mut emit);
        while self.reclaim_worker_for_raced_work() {
            self.drain_automatic_queue(&mut emit);
        }
    }

    fn drain_automatic_queue<F>(&self, emit: &mut F)
    where
        F: FnMut(ItemCountEvent),
    {
        while let Some(plan) = self.pop_automatic_plan() {
            self.process_automatic_request(plan, &mut *emit);
        }
    }

    fn pop_automatic_plan(&self) -> Option<AutomaticCountPlan> {
        self.automatic_queue
            .lock()
            .expect("item-count queue lock")
            .pop_front()
    }

    pub fn plan_automatic_request(&self, request: &VisibleItemCountsRequest) -> AutomaticCountPlan {
        self.plan_automatic_request_with_generation(request, request.context.request_id)
    }

    /// Plans viewport count work using the owning directory session's
    /// mutation-driven generation. Callers without a session retain the
    /// legacy request-id fallback, keeping this API safe for stale v1 flows.
    pub fn plan_automatic_request_with_generation(
        &self,
        request: &VisibleItemCountsRequest,
        generation: u64,
    ) -> AutomaticCountPlan {
        let mut automatic = self.automatic.lock().expect("item-count automatic lock");
        let scope_key = scope_key(&request.context);
        let state = automatic.entry(scope_key.clone()).or_default();
        let same_context = state
            .active_context
            .as_ref()
            .is_some_and(|context| contexts_match(context, &request.context));

        if !same_context {
            state.requested_paths.clear();
        }

        state.next_token += 1;
        let token = state.next_token;
        state.active_token = Some(token);
        state.active_context = Some(request.context.clone());

        // A new viewport supersedes the old plan. Carry reservations forward
        // into the replacement plan so removing its queued predecessor cannot
        // strand required unknown counts.
        let carried = state.requested_paths.clone();
        let remaining = AUTO_ITEM_COUNT_LIMIT.saturating_sub(carried.len());
        let selected = select_uncarried_paths(&request.paths, &carried, remaining);

        state.requested_paths = carried.clone();
        state.requested_paths.extend(selected.iter().cloned());

        let mut paths = carried;
        paths.extend(selected);

        AutomaticCountPlan {
            scope_key,
            token,
            context: request.context.clone(),
            generation,
            paths,
        }
    }

    pub fn is_automatic_cancelled(&self, plan: &AutomaticCountPlan) -> bool {
        let automatic = self.automatic.lock().expect("item-count automatic lock");
        automatic.get(&plan.scope_key).is_none_or(|state| {
            state.active_token != Some(plan.token)
                || state
                    .active_context
                    .as_ref()
                    .is_none_or(|context| !contexts_match(context, &plan.context))
        })
    }

    pub fn process_automatic_request<F>(&self, plan: AutomaticCountPlan, mut emit: F)
    where
        F: FnMut(ItemCountEvent),
    {
        if plan.paths.is_empty() {
            return;
        }

        let mut batch = AutomaticCountBatch::new(&plan);
        let flow = plan.paths.iter().enumerate().try_for_each(|(index, path)| {
            self.count_automatic_path(&plan, index, path, &mut batch, &mut emit)
        });
        if flow.is_break() {
            self.release_cancelled_automatic_paths(&plan);
            return;
        }

        let needs_terminal_event = !batch.emitted_any && !self.is_automatic_cancelled(&plan);
        if needs_terminal_event {
            emit(ItemCountEvent {
                context: plan.context,
                results: Vec::new(),
                done: true,
            });
        }
    }

    fn count_automatic_path<F>(
        &self,
        plan: &AutomaticCountPlan,
        index: usize,
        path: &str,
        batch: &mut AutomaticCountBatch,
        emit: &mut F,
    ) -> ControlFlow<()>
    where
        F: FnMut(ItemCountEvent),
    {
        if self.is_automatic_cancelled(plan) {
            return ControlFlow::Break(());
        }
        let state = self.resolve_item_count(path, plan.generation);
        let result = ItemCountResult {
            path: path.to_string(),
            item_count: state.value(),
        };
        if let Some(event) = batch.add(index, result) {
            emit(event);
        }
        ControlFlow::Continue(())
    }

    /// A cached exact or unavailable count is reused; equivalent in-flight
    /// work that owns the key is left to finish instead of recounting.
    fn resolve_item_count(&self, path: &str, generation: u64) -> ItemCountState {
        let mut cache = self.cache.lock().expect("item-count cache lock");
        let current = cache.state(path, generation);
        if matches!(
            current,
            ItemCountState::Exact { .. } | ItemCountState::Unavailable
        ) {
            return current;
        }
        if !cache.begin(path, generation) {
            return ItemCountState::Pending;
        }
        drop(cache);
        let resolved = match fs::read_item_count(Path::new(path)) {
            Some(value) => ItemCountState::Exact { value },
            None => ItemCountState::Unavailable,
        };
        let mut cache = self.cache.lock().expect("item-count cache lock");
        cache.resolve(path, generation, resolved.clone());
        resolved
    }

    fn release_cancelled_automatic_paths(&self, plan: &AutomaticCountPlan) {
        let mut automatic = self.automatic.lock().expect("item-count automatic lock");
        let Some(state) = automatic.get_mut(&plan.scope_key) else {
            return;
        };

        // A superseded plan never produces a terminal event for the frontend.
        // Remove its reservations so a later viewport request can retry its
        // still-unknown directories. Newer-plan paths are distinct because
        // planning occurred while these reservations were still present.
        state
            .requested_paths
            .retain(|path| !contains_path(&plan.paths, path));
    }

    pub fn sort_active_items(
        &self,
        request: &ActiveItemsSortRequest,
    ) -> Result<ActiveItemsSortResponse, FsError> {
        self.sort_active_items_with_session(request, None)
    }

    /// Same contract as [`Self::sort_active_items`], but when `sessions` is
    /// supplied and the requesting pane has an active
    /// [`crate::directory_session::DirectorySessionService`] session whose
    /// current path matches `request.context.path`, the sorted/filtered
    /// result is derived from that session's already-in-memory unfiltered
    /// snapshot instead of re-reading the directory from disk. Item counts
    /// for that derived path are resolved through [`ItemCountCache`] using
    /// the same state()/begin()/resolve() pattern `process_automatic_request`
    /// uses, so a directory whose count is already cached at the current
    /// generation is never recomputed.
    ///
    /// Falls back to the original full-read path (byte-for-byte, including
    /// eager `include_item_counts` population via `fs::list_dir_with_cancellation`)
    /// whenever there is no matching session — this never regresses
    /// correctness, it only skips the redundant work when the snapshot is
    /// provably current for the requested path.
    pub fn sort_active_items_with_session(
        &self,
        request: &ActiveItemsSortRequest,
        sessions: Option<&crate::directory_session::DirectorySessionService>,
    ) -> Result<ActiveItemsSortResponse, FsError> {
        let handle = self.begin_explicit_request(request);

        let session_snapshot = sessions.and_then(|sessions| {
            sessions.snapshot_for_pane_path(&request.context.pane_id, &request.context.path)
        });
        if let Some((snapshot, generation, resolved_path)) = session_snapshot {
            return self.sort_from_snapshot(request, &handle, snapshot, generation, resolved_path);
        }

        let outcome = fs::list_dir_with_cancellation(
            &ListDirOptions {
                path: request.context.path.clone(),
                sort_key: SortKey::Items,
                sort_direction: request.sort_direction,
                filter: request.filter.clone(),
                show_hidden: request.show_hidden,
                include_item_counts: true,
            },
            || self.is_explicit_cancelled(&handle),
        )?;

        match outcome {
            ListDirOutcome::Complete(response) if !self.is_explicit_cancelled(&handle) => {
                Ok(ActiveItemsSortResponse::Ready(ActiveItemsSortReady {
                    context: request.context.clone(),
                    path: response.path,
                    entries: response.entries,
                }))
            }
            ListDirOutcome::Complete(_) | ListDirOutcome::Cancelled => {
                Ok(superseded_sort_response(request))
            }
        }
    }

    /// Derives an `ActiveItemsSortResponse` from an already-in-memory session
    /// snapshot: filters/sorts it with the exact same semantics
    /// `directory_session::view::SessionView::derive` uses (reused directly,
    /// so a session-derived Items sort and a session-derived Name/Size/etc.
    /// sort never visibly disagree on filter/hidden-file rules), then fills
    /// in each directory entry's `item_count` from `ItemCountCache`,
    /// computing on a cache miss via the same non-batch `fs::read_item_count`
    /// call `process_automatic_request` uses (never the eager thread-pool
    /// `fs::populate_item_counts_with_cancellation` path, since that would
    /// reintroduce the redundant-recount cost this method exists to avoid).
    fn sort_from_snapshot(
        &self,
        request: &ActiveItemsSortRequest,
        handle: &ExplicitCountHandle,
        snapshot: Vec<DirectoryEntry>,
        generation: u64,
        resolved_path: String,
    ) -> Result<ActiveItemsSortResponse, FsError> {
        use crate::directory_session::model::ViewParams;
        use crate::directory_session::view::SessionView;

        let view_params = ViewParams {
            sort_key: SortKey::Items,
            sort_direction: request.sort_direction,
            filter: request.filter.clone(),
            show_hidden: request.show_hidden,
            include_item_counts: true,
        };
        let view = SessionView::derive(&snapshot, &view_params);
        let mut entries: Vec<DirectoryEntry> = view.rows().to_vec();

        let flow = entries
            .iter_mut()
            .try_for_each(|entry| self.fill_directory_item_count(entry, handle, generation));
        if flow.is_break() {
            return Ok(superseded_sort_response(request));
        }

        // Re-sort now that item counts (the active sort key's field) may
        // have been filled in above — SessionView::derive sorted using
        // whatever counts the snapshot already carried, which is frequently
        // `None` for entries never previously counted.
        entries.sort_by(|left, right| {
            fs::compare_entries(left, right, SortKey::Items, request.sort_direction)
        });

        if self.is_explicit_cancelled(handle) {
            return Ok(superseded_sort_response(request));
        }

        Ok(ActiveItemsSortResponse::Ready(ActiveItemsSortReady {
            context: request.context.clone(),
            path: resolved_path,
            entries,
        }))
    }

    fn fill_directory_item_count(
        &self,
        entry: &mut DirectoryEntry,
        handle: &ExplicitCountHandle,
        generation: u64,
    ) -> ControlFlow<()> {
        if self.is_explicit_cancelled(handle) {
            return ControlFlow::Break(());
        }
        if !entry.is_dir {
            return ControlFlow::Continue(());
        }
        let state = self.resolve_item_count(&entry.path, generation);
        if let Some(value) = state.value() {
            entry.item_count = Some(value);
        }
        ControlFlow::Continue(())
    }

    /// Called by watch/session owners after an accepted direct-child patch.
    /// The directory itself is the only count whose generation becomes stale.
    pub fn invalidate_directory_generation(&self, path: &str, generation: u64) {
        self.cache
            .lock()
            .expect("item-count cache lock")
            .invalidate_generation(path, generation);
    }

    #[cfg(feature = "test-utils")]
    pub fn cache_len(&self) -> usize {
        self.cache.lock().expect("item-count cache lock").len()
    }

    #[cfg(feature = "test-utils")]
    pub fn record_test_event(&self, event: ItemCountEvent) {
        self.test_events
            .lock()
            .expect("item-count test event lock")
            .push(event);
    }

    #[cfg(feature = "test-utils")]
    pub fn take_test_events(&self) -> Vec<ItemCountEvent> {
        std::mem::take(&mut *self.test_events.lock().expect("item-count test event lock"))
    }

    #[cfg(feature = "test-utils")]
    pub fn automatic_queue_len(&self) -> usize {
        self.automatic_queue
            .lock()
            .expect("item-count queue lock")
            .len()
    }

    fn begin_explicit_request(&self, request: &ActiveItemsSortRequest) -> ExplicitCountHandle {
        let mut explicit = self.explicit.lock().expect("item-count explicit lock");
        let scope_key = scope_key(&request.context);
        let state = explicit.entry(scope_key.clone()).or_default();
        state.next_token += 1;
        let token = state.next_token;
        state.active_token = Some(token);
        state.active_context = Some(request.context.clone());

        ExplicitCountHandle {
            scope_key,
            token,
            context: request.context.clone(),
        }
    }

    fn is_explicit_cancelled(&self, handle: &ExplicitCountHandle) -> bool {
        let explicit = self.explicit.lock().expect("item-count explicit lock");
        explicit.get(&handle.scope_key).is_none_or(|state| {
            state.active_token != Some(handle.token)
                || state
                    .active_context
                    .as_ref()
                    .is_none_or(|context| !contexts_match(context, &handle.context))
        })
    }
}

fn scope_key(context: &ItemCountRequestContext) -> String {
    format!("{}\n{}", context.pane_id, context.tab_id)
}

fn contexts_match(left: &ItemCountRequestContext, right: &ItemCountRequestContext) -> bool {
    left.request_id == right.request_id
        && left.pane_id == right.pane_id
        && left.tab_id == right.tab_id
        && strings_match(&left.path, &right.path)
}

struct AutomaticCountBatch {
    context: ItemCountRequestContext,
    path_count: usize,
    results: Vec<ItemCountResult>,
    emitted_any: bool,
}

impl AutomaticCountBatch {
    fn new(plan: &AutomaticCountPlan) -> Self {
        Self {
            context: plan.context.clone(),
            path_count: plan.paths.len(),
            results: Vec::new(),
            emitted_any: false,
        }
    }

    fn add(&mut self, index: usize, result: ItemCountResult) -> Option<ItemCountEvent> {
        self.results.push(result);
        let is_last = index + 1 == self.path_count;
        let is_full = self.results.len() >= ITEM_COUNT_BATCH_SIZE;
        let should_emit = is_full || is_last;
        if !should_emit {
            return None;
        }
        self.emitted_any = true;
        Some(ItemCountEvent {
            context: self.context.clone(),
            results: std::mem::take(&mut self.results),
            done: is_last,
        })
    }
}

fn select_uncarried_paths(candidates: &[String], carried: &[String], limit: usize) -> Vec<String> {
    let mut selected = Vec::new();
    for path in candidates {
        if selected.len() >= limit {
            break;
        }
        let already_planned = contains_path(carried, path) || contains_path(&selected, path);
        if !already_planned {
            selected.push(path.clone());
        }
    }
    selected
}

fn superseded_sort_response(request: &ActiveItemsSortRequest) -> ActiveItemsSortResponse {
    ActiveItemsSortResponse::Superseded(ActiveItemsSortSuperseded {
        context: request.context.clone(),
    })
}

fn contains_path(paths: &[String], candidate: &str) -> bool {
    paths.iter().any(|path| strings_match(path, candidate))
}

fn strings_match(left: &str, right: &str) -> bool {
    left == right || left.eq_ignore_ascii_case(right)
}
