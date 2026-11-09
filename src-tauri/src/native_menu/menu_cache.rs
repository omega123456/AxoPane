use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use super::provider::{ProviderInvocation, ProviderNativeMenuItem};
use super::types::{LoadNativeMenuRequest, NativeMenuTargetKind};
use crate::bounded_cache::{BoundedCache, Weighted};

const MAX_CACHE_ENTRIES: usize = 128;
const MAX_CACHE_WEIGHT: usize = 16 * 1024 * 1024;

pub struct CacheLoad {
    pub items: Vec<ProviderNativeMenuItem>,
    pub loaded: bool,
}

pub struct MenuCache {
    entries: Mutex<BoundedCache<String, Vec<ProviderNativeMenuItem>>>,
    in_flight: Mutex<HashMap<String, Arc<CacheFlight>>>,
}

struct CacheFlight {
    result: Mutex<Option<Vec<ProviderNativeMenuItem>>>,
    ready: Condvar,
}

enum Flight {
    Leader(Arc<CacheFlight>),
    Follower(Arc<CacheFlight>),
}

impl CacheFlight {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn publish(&self, structure: Vec<ProviderNativeMenuItem>) {
        *self.result.lock().expect("menu cache flight result lock") = Some(structure);
        self.ready.notify_all();
    }

    fn publish_empty_if_unpublished(&self) {
        self.result
            .lock()
            .expect("menu cache flight result lock")
            .get_or_insert_with(Vec::new);
        self.ready.notify_all();
    }

    fn read_when_ready<R>(&self, read: impl FnOnce(&[ProviderNativeMenuItem]) -> R) -> R {
        let mut result = self.result.lock().expect("menu cache flight result lock");
        while result.is_none() {
            result = self.ready.wait(result).expect("menu cache flight wait");
        }
        read(result.as_ref().expect("menu cache flight result"))
    }
}

struct LeadingFlight<'flight> {
    cache: &'flight MenuCache,
    flight: &'flight CacheFlight,
    key: &'flight String,
}

impl LeadingFlight<'_> {
    fn publish_loaded<F>(&self, request: &LoadNativeMenuRequest, load: F)
    where
        F: FnOnce() -> Vec<ProviderNativeMenuItem>,
    {
        self.flight
            .publish(self.cache.load_structure(request, load));
    }
}

impl Drop for LeadingFlight<'_> {
    fn drop(&mut self) {
        self.flight.publish_empty_if_unpublished();
        self.cache.finish_flight(self.key);
    }
}

impl Flight {
    fn is_leader(&self) -> bool {
        matches!(self, Self::Leader(_))
    }

    fn shared(&self) -> &CacheFlight {
        match self {
            Self::Leader(flight) | Self::Follower(flight) => flight,
        }
    }
}

impl Default for MenuCache {
    fn default() -> Self {
        Self {
            entries: Mutex::new(BoundedCache::new(MAX_CACHE_ENTRIES, MAX_CACHE_WEIGHT)),
            in_flight: Mutex::new(HashMap::new()),
        }
    }
}

impl MenuCache {
    pub fn get(&self, request: &LoadNativeMenuRequest) -> Option<Vec<ProviderNativeMenuItem>> {
        let key = cache_key(request);
        let mut entries = self.entries.lock().expect("menu cache lock");
        entries.get(&key).map(|items| rebind_items(items, request))
    }

    pub fn insert(&self, request: &LoadNativeMenuRequest, items: &[ProviderNativeMenuItem]) {
        let key = cache_key(request);
        let mut entries = self.entries.lock().expect("menu cache lock");
        let entry = Weighted {
            weight: estimated_weight(&key, items),
            value: structuralize_items(items),
        };
        entries.insert(key, entry);
    }

    pub fn get_or_load<F>(&self, request: &LoadNativeMenuRequest, load: F) -> CacheLoad
    where
        F: FnOnce() -> Vec<ProviderNativeMenuItem>,
    {
        if let Some(items) = self.get(request) {
            return CacheLoad {
                items,
                loaded: false,
            };
        }

        let key = cache_key(request);
        let flight = self.join_flight(&key);
        if flight.is_leader() {
            let leading = LeadingFlight {
                cache: self,
                flight: flight.shared(),
                key: &key,
            };
            leading.publish_loaded(request, load);
        }
        let items = flight
            .shared()
            .read_when_ready(|structure| rebind_items(structure, request));
        CacheLoad {
            items,
            loaded: flight.is_leader(),
        }
    }

    fn join_flight(&self, key: &String) -> Flight {
        let mut flights = self.in_flight.lock().expect("menu cache flights lock");
        if let Some(flight) = flights.get(key) {
            return Flight::Follower(Arc::clone(flight));
        }
        let flight = Arc::new(CacheFlight::new());
        flights.insert(key.clone(), Arc::clone(&flight));
        Flight::Leader(flight)
    }

    fn load_structure<F>(
        &self,
        request: &LoadNativeMenuRequest,
        load: F,
    ) -> Vec<ProviderNativeMenuItem>
    where
        F: FnOnce() -> Vec<ProviderNativeMenuItem>,
    {
        let fresh = load();
        self.insert(request, &fresh);
        structuralize_items(&fresh)
    }

    fn finish_flight(&self, key: &String) {
        self.in_flight
            .lock()
            .expect("menu cache flights lock")
            .remove(key);
    }

    pub fn len(&self) -> usize {
        self.entries.lock().expect("menu cache lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, request: &LoadNativeMenuRequest) -> bool {
        let key = cache_key(request);
        self.entries
            .lock()
            .expect("menu cache lock")
            .get(&key)
            .is_some()
    }
}

fn estimated_weight(key: &str, items: &[ProviderNativeMenuItem]) -> usize {
    key.len() + items.iter().map(item_weight).sum::<usize>()
}

fn item_weight(item: &ProviderNativeMenuItem) -> usize {
    item.id.len()
        + item.label.len()
        + item.normalized_verb.as_ref().map_or(0, String::len)
        + item.icon.as_ref().map_or(0, |icon| {
            icon.data_url.len() + icon.alt.as_ref().map_or(0, String::len)
        })
        + item.children.iter().map(item_weight).sum::<usize>()
}

pub(crate) fn cache_key(request: &LoadNativeMenuRequest) -> String {
    format!(
        "{}::{}",
        target_kind_key(&request.target_kind),
        type_signature(&request.selected_paths)
    )
}

fn target_kind_key(kind: &NativeMenuTargetKind) -> &'static str {
    match kind {
        NativeMenuTargetKind::File => "file",
        NativeMenuTargetKind::Folder => "folder",
        NativeMenuTargetKind::Multi => "multi",
        NativeMenuTargetKind::Mixed => "mixed",
        NativeMenuTargetKind::DriveRoot => "driveRoot",
        NativeMenuTargetKind::Background => "background",
        NativeMenuTargetKind::Tree => "tree",
        NativeMenuTargetKind::Tab => "tab",
    }
}

fn type_signature(selected_paths: &[String]) -> String {
    let mut extensions: Vec<String> = selected_paths
        .iter()
        .map(|path| extension_of(path))
        .collect();
    extensions.sort();
    extensions.dedup();
    extensions.join("|")
}

fn extension_of(path: &str) -> String {
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);

    let Some((stem, extension)) = file_name.rsplit_once('.') else {
        return String::new();
    };
    if stem.is_empty() {
        return String::new();
    }
    extension.to_ascii_lowercase()
}

fn rebind_items(
    items: &[ProviderNativeMenuItem],
    request: &LoadNativeMenuRequest,
) -> Vec<ProviderNativeMenuItem> {
    items
        .iter()
        .map(|item| rebind_item(item, request))
        .collect()
}

fn structuralize_items(items: &[ProviderNativeMenuItem]) -> Vec<ProviderNativeMenuItem> {
    items.iter().map(structuralize_item).collect()
}

fn structuralize_item(item: &ProviderNativeMenuItem) -> ProviderNativeMenuItem {
    ProviderNativeMenuItem {
        id: item.id.clone(),
        label: item.label.clone(),
        enabled: item.enabled,
        danger: item.danger,
        canonical_action_kind: item.canonical_action_kind.clone(),
        normalized_verb: item.normalized_verb.clone(),
        icon: item.icon.clone(),
        invocation: item.invocation.as_ref().map(structuralize_invocation),
        children: structuralize_items(&item.children),
    }
}

fn structuralize_invocation(invocation: &ProviderInvocation) -> ProviderInvocation {
    match invocation {
        ProviderInvocation::Windows {
            request,
            command_path,
        } => ProviderInvocation::Windows {
            request: structural_request(request),
            command_path: command_path.clone(),
        },
        ProviderInvocation::WindowsModern {
            clsid,
            packaged,
            request,
            command_path,
        } => ProviderInvocation::WindowsModern {
            clsid: *clsid,
            packaged: *packaged,
            request: structural_request(request),
            command_path: command_path.clone(),
        },
        ProviderInvocation::Fake { action_id } => ProviderInvocation::Fake {
            action_id: action_id.clone(),
        },
    }
}

fn structural_request(request: &LoadNativeMenuRequest) -> LoadNativeMenuRequest {
    LoadNativeMenuRequest {
        request_id: String::new(),
        target_kind: request.target_kind.clone(),
        target_path: None,
        folder_path: None,
        selected_paths: Vec::new(),
    }
}

fn rebind_item(
    item: &ProviderNativeMenuItem,
    request: &LoadNativeMenuRequest,
) -> ProviderNativeMenuItem {
    ProviderNativeMenuItem {
        id: item.id.clone(),
        label: item.label.clone(),
        enabled: item.enabled,
        danger: item.danger,
        canonical_action_kind: item.canonical_action_kind.clone(),
        normalized_verb: item.normalized_verb.clone(),
        icon: item.icon.clone(),
        invocation: item
            .invocation
            .as_ref()
            .map(|invocation| rebind_invocation(invocation, request)),
        children: rebind_items(&item.children, request),
    }
}

fn rebind_invocation(
    invocation: &ProviderInvocation,
    request: &LoadNativeMenuRequest,
) -> ProviderInvocation {
    match invocation {
        ProviderInvocation::Windows { command_path, .. } => ProviderInvocation::Windows {
            request: request.clone(),
            command_path: command_path.clone(),
        },
        ProviderInvocation::WindowsModern {
            clsid,
            packaged,
            command_path,
            ..
        } => ProviderInvocation::WindowsModern {
            clsid: *clsid,
            packaged: *packaged,
            request: request.clone(),
            command_path: command_path.clone(),
        },
        ProviderInvocation::Fake { action_id } => ProviderInvocation::Fake {
            action_id: action_id.clone(),
        },
    }
}
