use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use serde::{Deserialize, Serialize};

use crate::fs::{self, DirectoryEntry, ListDirOptions, SortDirection, SortKey};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryPatch {
    pub path: String,
    pub entry: Option<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirPatch {
    pub tab_id: String,
    pub path: String,
    pub reason: String,
    pub changed: Vec<DirEntryPatch>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WatchTarget {
    pub tab_id: String,
    pub path: String,
    pub sort_key: SortKey,
    pub sort_direction: SortDirection,
    pub filter: String,
    pub show_hidden: bool,
}

#[derive(Default)]
pub struct WatchService {
    inner: Mutex<Option<WatchRuntime>>,
}

struct WatchRuntime {
    debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
    tabs: Arc<Mutex<HashMap<String, WatchedTab>>>,
    watch_counts: HashMap<PathBuf, usize>,
}

#[derive(Clone)]
struct WatchedTab {
    target: WatchTarget,
    snapshot: HashMap<String, DirectoryEntry>,
}

impl WatchService {
    pub fn set_tab_watch<FPatch, FError>(&self, target: Option<WatchTarget>, emit_patch: FPatch, emit_error: FError) -> Result<(), String>
    where
        FPatch: Fn(DirPatch) + Send + Sync + 'static,
        FError: Fn(String, String) + Send + Sync + 'static,
    {
        let mut guard = self.inner.lock().expect("watch service lock");

        if let Some(target) = target {
            let runtime = guard.get_or_insert_with(|| create_runtime(emit_patch, emit_error).expect("watch runtime"));
            let path = PathBuf::from(&target.path);
            let snapshot = snapshot_for_target(&target)?;
            let pane_name = pane_scope(&target.tab_id).to_string();

            let (same_tab_previous, stale_tabs) = {
                let tabs = runtime.tabs.lock().expect("watch tabs lock");
                let same_tab_previous = tabs.get(&target.tab_id).cloned();
                let stale_tabs = tabs
                    .iter()
                    .filter(|(tab_id, _)| pane_scope(tab_id) == pane_name && *tab_id != &target.tab_id)
                    .map(|(tab_id, watched)| (tab_id.clone(), watched.clone()))
                    .collect::<Vec<_>>();
                (same_tab_previous, stale_tabs)
            };

            if same_tab_previous
                .as_ref()
                .is_none_or(|previous| previous.target.path != target.path)
            {
                add_watch(runtime, &path)?;
            }

            if let Some(previous) = same_tab_previous {
                if previous.target.path != target.path {
                    remove_watch(runtime, Path::new(&previous.target.path))?;
                }
            }

            for (_, stale) in &stale_tabs {
                remove_watch(runtime, Path::new(&stale.target.path))?;
            }

            let mut tabs = runtime.tabs.lock().expect("watch tabs lock");
            for (stale_tab_id, _) in stale_tabs {
                tabs.remove(&stale_tab_id);
            }
            tabs.insert(target.tab_id.clone(), WatchedTab { target, snapshot });
        } else if let Some(runtime) = guard.as_mut() {
            let watched_paths = {
                let tabs = runtime.tabs.lock().expect("watch tabs lock");
                tabs.values()
                    .map(|tab| PathBuf::from(&tab.target.path))
                    .collect::<Vec<_>>()
            };
            for watched_path in watched_paths {
                remove_watch(runtime, &watched_path)?;
            }
            runtime.tabs.lock().expect("watch tabs lock").clear();
        }

        Ok(())
    }

    pub fn refresh_tab<FPatch>(&self, target: WatchTarget, emit_patch: FPatch) -> Result<DirPatch, String>
    where
        FPatch: Fn(DirPatch) + Send + Sync + 'static,
    {
        let mut guard = self.inner.lock().expect("watch service lock");
        let runtime = guard
            .get_or_insert_with(|| create_runtime(emit_patch, |_, _| {}).expect("watch runtime"));

        let mut tabs = runtime.tabs.lock().expect("watch tabs lock");
        let previous = tabs.get(&target.tab_id).map(|tab| tab.snapshot.clone()).unwrap_or_default();
        let next = snapshot_for_target(&target)?;
        let patch = diff_entries(&target.tab_id, &target.path, "refresh", &previous, &next);

        tabs.insert(
            target.tab_id.clone(),
            WatchedTab {
                target,
                snapshot: next,
            },
        );

        Ok(patch)
    }
}

fn create_runtime<FPatch, FError>(emit_patch: FPatch, emit_error: FError) -> Result<WatchRuntime, String>
where
    FPatch: Fn(DirPatch) + Send + Sync + 'static,
    FError: Fn(String, String) + Send + Sync + 'static,
{
    let tabs = Arc::new(Mutex::new(HashMap::<String, WatchedTab>::new()));
    let patch_emitter = Arc::new(emit_patch);
    let error_emitter = Arc::new(emit_error);
    let tabs_for_callback = tabs.clone();

    let debouncer = new_debouncer(
        Duration::from_millis(150),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                let changed_paths: HashSet<PathBuf> = events
                    .iter()
                    .flat_map(|event| event.paths.iter().cloned())
                    .filter(|path| path.parent().is_some())
                    .collect();

                let mut tabs = tabs_for_callback.lock().expect("watch tabs lock");
                for watched in tabs.values_mut() {
                    if !changed_paths
                        .iter()
                        .any(|changed_path| changed_path.parent() == Some(Path::new(&watched.target.path)))
                    {
                        continue;
                    }

                    match snapshot_for_target(&watched.target) {
                        Ok(next_snapshot) => {
                            let patch = diff_entries(
                                &watched.target.tab_id,
                                &watched.target.path,
                                "watch",
                                &watched.snapshot,
                                &next_snapshot,
                            );
                            watched.snapshot = next_snapshot;
                            if !patch.changed.is_empty() || !patch.removed.is_empty() {
                                patch_emitter(patch);
                            }
                        }
                        Err(error) => error_emitter(watched.target.path.clone(), error),
                    }
                }
            }
            Err(errors) => {
                for error in errors {
                    error_emitter(
                        error
                            .paths
                            .first()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        error.to_string(),
                    );
                }
            }
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(WatchRuntime {
        debouncer,
        tabs,
        watch_counts: HashMap::new(),
    })
}

fn pane_scope(tab_id: &str) -> &str {
    tab_id.split_once('-').map_or(tab_id, |(scope, _)| scope)
}

fn add_watch(runtime: &mut WatchRuntime, path: &Path) -> Result<(), String> {
    match runtime.watch_counts.get_mut(path) {
        Some(count) => {
            *count += 1;
            Ok(())
        }
        None => {
            runtime
                .debouncer
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(|error| error.to_string())?;
            runtime.watch_counts.insert(path.to_path_buf(), 1);
            Ok(())
        }
    }
}

fn remove_watch(runtime: &mut WatchRuntime, path: &Path) -> Result<(), String> {
    let Some(count) = runtime.watch_counts.get_mut(path) else {
        return Ok(());
    };

    if *count > 1 {
        *count -= 1;
        return Ok(());
    }

    runtime
        .debouncer
        .unwatch(path)
        .map_err(|error| error.to_string())?;
    runtime.watch_counts.remove(path);
    Ok(())
}

fn snapshot_for_target(target: &WatchTarget) -> Result<HashMap<String, DirectoryEntry>, String> {
    let response = fs::list_dir(&ListDirOptions {
        path: target.path.clone(),
        sort_key: target.sort_key,
        sort_direction: target.sort_direction,
        filter: target.filter.clone(),
        show_hidden: target.show_hidden,
    })
    .map_err(|error| error.to_string())?;

    Ok(response
        .entries
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect())
}

fn diff_entries(
    tab_id: &str,
    path: &str,
    reason: &str,
    previous: &HashMap<String, DirectoryEntry>,
    next: &HashMap<String, DirectoryEntry>,
) -> DirPatch {
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for (entry_path, entry) in next {
        if previous.get(entry_path) != Some(entry) {
            changed.push(DirEntryPatch {
                path: entry_path.clone(),
                entry: Some(entry.clone()),
            });
        }
    }

    for entry_path in previous.keys() {
        if !next.contains_key(entry_path) {
            removed.push(entry_path.clone());
        }
    }

    DirPatch {
        tab_id: tab_id.to_string(),
        path: path.to_string(),
        reason: reason.to_string(),
        changed,
        removed,
    }
}
