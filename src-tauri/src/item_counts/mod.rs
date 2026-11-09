use std::collections::{HashMap, VecDeque};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::fs::{
    self, DirectoryEntry, FsError, ListDirOptions, ListDirOutcome, SortDirection, SortKey,
};

pub mod cache;
use crate::directory_session::PaneSnapshot;
use cache::{CountResolution, ItemCountCache, ItemCountState};

pub const AUTO_ITEM_COUNT_LIMIT: usize = 200;
pub const ITEM_COUNT_BATCH_SIZE: usize = 64;
pub const MAX_AUTOMATIC_ITEM_COUNT_WORKERS: usize = 2;
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

    pub fn enqueue_automatic_request(&self, plan: AutomaticCountPlan) -> bool {
        if plan.is_empty() {
            return false;
        }
        let mut queue = self.automatic_queue.lock().expect("item-count queue lock");
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

        let mut run = AutomaticCountRun {
            plan: &plan,
            batch: AutomaticCountBatch::new(&plan),
            emit: &mut emit,
        };
        let flow = plan.paths.iter().enumerate().try_for_each(|(index, path)| {
            self.count_automatic_path(&mut run, IndexedPath { index, path })
        });
        if flow.is_break() {
            self.release_cancelled_automatic_paths(&plan);
            return;
        }

        let emitted_any = run.batch.emitted_any;
        let needs_terminal_event = !emitted_any && !self.is_automatic_cancelled(&plan);
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
        run: &mut AutomaticCountRun<'_, F>,
        item: IndexedPath<'_>,
    ) -> ControlFlow<()>
    where
        F: FnMut(ItemCountEvent),
    {
        if self.is_automatic_cancelled(run.plan) {
            return ControlFlow::Break(());
        }
        let state = self.resolve_item_count(item.path, run.plan.generation);
        let result = ItemCountResult {
            path: item.path.to_string(),
            item_count: state.value(),
        };
        if let Some(event) = run.batch.add(item.index, result) {
            (run.emit)(event);
        }
        ControlFlow::Continue(())
    }

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
        cache.resolve(CountResolution {
            path,
            generation,
            state: resolved.clone(),
        });
        resolved
    }

    fn release_cancelled_automatic_paths(&self, plan: &AutomaticCountPlan) {
        let mut automatic = self.automatic.lock().expect("item-count automatic lock");
        let Some(state) = automatic.get_mut(&plan.scope_key) else {
            return;
        };

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

    pub fn sort_active_items_with_session(
        &self,
        request: &ActiveItemsSortRequest,
        sessions: Option<&crate::directory_session::DirectorySessionService>,
    ) -> Result<ActiveItemsSortResponse, FsError> {
        let handle = self.begin_explicit_request(request);

        let session_snapshot = sessions.and_then(|sessions| {
            sessions.snapshot_for_pane_path(&request.context.pane_id, &request.context.path)
        });
        if let Some(snapshot) = session_snapshot {
            let sort = ExplicitSort {
                request,
                handle: &handle,
            };
            return self.sort_from_snapshot(sort, snapshot);
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

    fn sort_from_snapshot(
        &self,
        sort: ExplicitSort<'_>,
        session_snapshot: PaneSnapshot,
    ) -> Result<ActiveItemsSortResponse, FsError> {
        let ExplicitSort { request, handle } = sort;
        let PaneSnapshot {
            entries: snapshot,
            generation,
            resolved_path,
        } = session_snapshot;
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

        let pass = ExplicitCountPass { handle, generation };
        let flow = entries
            .iter_mut()
            .try_for_each(|entry| self.fill_directory_item_count(entry, pass));
        if flow.is_break() {
            return Ok(superseded_sort_response(request));
        }

        let items_order = fs::SortOrder {
            key: SortKey::Items,
            direction: request.sort_direction,
        };
        entries.sort_by(|left, right| fs::compare_entries(left, right, items_order));

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
        pass: ExplicitCountPass<'_>,
    ) -> ControlFlow<()> {
        if self.is_explicit_cancelled(pass.handle) {
            return ControlFlow::Break(());
        }
        if !entry.is_dir {
            return ControlFlow::Continue(());
        }
        let state = self.resolve_item_count(&entry.path, pass.generation);
        if let Some(value) = state.value() {
            entry.item_count = Some(value);
        }
        ControlFlow::Continue(())
    }

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

struct AutomaticCountRun<'run, F> {
    plan: &'run AutomaticCountPlan,
    batch: AutomaticCountBatch,
    emit: &'run mut F,
}

struct IndexedPath<'path> {
    index: usize,
    path: &'path str,
}

#[derive(Clone, Copy)]
struct ExplicitSort<'sort> {
    request: &'sort ActiveItemsSortRequest,
    handle: &'sort ExplicitCountHandle,
}

#[derive(Clone, Copy)]
struct ExplicitCountPass<'pass> {
    handle: &'pass ExplicitCountHandle,
    generation: u64,
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
