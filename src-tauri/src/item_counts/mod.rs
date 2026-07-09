use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::fs::{self, DirectoryEntry, FsError, ListDirOptions, ListDirOutcome, SortDirection, SortKey};

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

#[derive(Debug, Default)]
pub struct ItemCountService {
    automatic: Mutex<HashMap<String, AutomaticScopeState>>,
    explicit: Mutex<HashMap<String, ExplicitScopeState>>,
    automatic_queue: Mutex<VecDeque<AutomaticCountPlan>>,
    automatic_workers: Mutex<usize>,
    #[cfg(feature = "test-utils")]
    test_events: Mutex<Vec<ItemCountEvent>>,
}

impl ItemCountService {
    pub fn cancel_tab(&self, tab_id: &str) {
        self.automatic.lock().expect("item-count automatic lock").retain(|_, state| {
            state.active_context.as_ref().is_none_or(|context| context.tab_id != tab_id)
        });
        self.explicit.lock().expect("item-count explicit lock").retain(|_, state| {
            state.active_context.as_ref().is_none_or(|context| context.tab_id != tab_id)
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
        let mut workers = self.automatic_workers.lock().expect("item-count workers lock");
        if *workers >= MAX_AUTOMATIC_ITEM_COUNT_WORKERS { return false; }
        *workers += 1;
        true
    }

    pub fn process_automatic_queue<F>(&self, mut emit: F)
    where F: FnMut(ItemCountEvent) {
        loop {
            let plan = self.automatic_queue.lock().expect("item-count queue lock").pop_front();
            let Some(plan) = plan else {
                {
                    let mut workers = self.automatic_workers.lock().expect("item-count workers lock");
                    *workers -= 1;
                }
                // An enqueue can race the empty-pop above while all workers
                // are winding down. Reclaim this thread when work appeared;
                // otherwise a later enqueue observes the released slot and
                // starts a bounded replacement worker.
                if self.automatic_queue.lock().expect("item-count queue lock").is_empty() {
                    return;
                }
                let mut workers = self.automatic_workers.lock().expect("item-count workers lock");
                if *workers >= MAX_AUTOMATIC_ITEM_COUNT_WORKERS {
                    return;
                }
                *workers += 1;
                continue;
            };
            self.process_automatic_request(plan, &mut emit);
        }
    }
    pub fn plan_automatic_request(&self, request: &VisibleItemCountsRequest) -> AutomaticCountPlan {
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
        let carried = if same_context {
            state.requested_paths.clone()
        } else {
            Vec::new()
        };
        let remaining = AUTO_ITEM_COUNT_LIMIT.saturating_sub(carried.len());
        let mut selected = Vec::new();
        for path in &request.paths {
            if selected.len() >= remaining {
                break;
            }

            if contains_path(&carried, path) || contains_path(&selected, path) {
                continue;
            }

            selected.push(path.clone());
        }

        state.requested_paths = carried.clone();
        state.requested_paths.extend(selected.iter().cloned());

        let mut paths = carried;
        paths.extend(selected);

        AutomaticCountPlan {
            scope_key,
            token,
            context: request.context.clone(),
            paths,
        }
    }

    pub fn is_automatic_cancelled(&self, plan: &AutomaticCountPlan) -> bool {
        let automatic = self.automatic.lock().expect("item-count automatic lock");
        automatic
            .get(&plan.scope_key)
            .is_none_or(|state| {
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

        let mut batch = Vec::new();
        let mut emitted_any = false;
        for (index, path) in plan.paths.iter().enumerate() {
            if self.is_automatic_cancelled(&plan) {
                self.release_cancelled_automatic_paths(&plan);
                return;
            }

            batch.push(ItemCountResult {
                path: path.clone(),
                item_count: fs::read_item_count(Path::new(path)),
            });

            let is_last = index + 1 == plan.paths.len();
            if batch.len() >= ITEM_COUNT_BATCH_SIZE || is_last {
                let results = std::mem::take(&mut batch);
                let event = ItemCountEvent {
                    context: plan.context.clone(),
                    results,
                    done: is_last,
                };
                emit(event);
                emitted_any = true;
            }
        }

        if !emitted_any && !self.is_automatic_cancelled(&plan) {
            emit(ItemCountEvent {
                context: plan.context,
                results: Vec::new(),
                done: true,
            });
        }
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
        let handle = self.begin_explicit_request(request);
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
            ListDirOutcome::Complete(response) => {
                if self.is_explicit_cancelled(&handle) {
                    Ok(ActiveItemsSortResponse::Superseded(
                        ActiveItemsSortSuperseded {
                            context: request.context.clone(),
                        },
                    ))
                } else {
                    Ok(ActiveItemsSortResponse::Ready(ActiveItemsSortReady {
                        context: request.context.clone(),
                        path: response.path,
                        entries: response.entries,
                    }))
                }
            }
            ListDirOutcome::Cancelled => Ok(ActiveItemsSortResponse::Superseded(
                ActiveItemsSortSuperseded {
                    context: request.context.clone(),
                },
            )),
        }
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
        std::mem::take(
            &mut *self
                .test_events
                .lock()
                .expect("item-count test event lock"),
        )
    }

    #[cfg(feature = "test-utils")]
    pub fn automatic_queue_len(&self) -> usize {
        self.automatic_queue.lock().expect("item-count queue lock").len()
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
        explicit
            .get(&handle.scope_key)
            .is_none_or(|state| {
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

fn contains_path(paths: &[String], candidate: &str) -> bool {
    paths.iter().any(|path| strings_match(path, candidate))
}

fn strings_match(left: &str, right: &str) -> bool {
    crate::listing::paths_match_for_context(left, right)
}
