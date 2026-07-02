use std::collections::HashMap;
use std::sync::Mutex;

use super::provider::{ProviderInvocation, ProviderNativeMenuItem};
use super::types::{LoadNativeMenuRequest, NativeMenuTargetKind};

/// In-memory, per-file-type cache for the (expensive) native shell-menu
/// enumeration. Entries are keyed on the menu's *structure inputs* — the target
/// kind plus the set of file extensions in the selection — never on the concrete
/// paths. The cached menu structure is reused across files of the same type;
/// only the path-bound parts of each invocation are rebound to the live request
/// on retrieval (see [`rebind_items`]).
///
/// The cache lives for the lifetime of the [`super::NativeMenuService`], which is
/// a single app-managed instance, so it is naturally discarded on app restart.
#[derive(Default)]
pub struct MenuCache {
    entries: Mutex<HashMap<String, Vec<ProviderNativeMenuItem>>>,
}

impl MenuCache {
    /// Returns the cached menu for the request's file type, with each
    /// invocation rebound to the request's live paths, or `None` on a miss.
    pub fn get(&self, request: &LoadNativeMenuRequest) -> Option<Vec<ProviderNativeMenuItem>> {
        let key = cache_key(request);
        let entries = self.entries.lock().expect("menu cache lock");
        entries.get(&key).map(|items| rebind_items(items, request))
    }

    /// Stores the (already deduped) menu structure for the request's file type.
    pub fn insert(&self, request: &LoadNativeMenuRequest, items: &[ProviderNativeMenuItem]) {
        let key = cache_key(request);
        let mut entries = self.entries.lock().expect("menu cache lock");
        entries.insert(key, items.to_vec());
    }

    /// Number of distinct file-type entries currently cached.
    pub fn len(&self) -> usize {
        self.entries.lock().expect("menu cache lock").len()
    }

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cheap membership check on the request's cache key — no path-rebind
    /// work. Used by the background warm path to skip a type that is already
    /// cached without paying `get`'s per-item clone/rebind cost.
    pub fn contains(&self, request: &LoadNativeMenuRequest) -> bool {
        let key = cache_key(request);
        self.entries
            .lock()
            .expect("menu cache lock")
            .contains_key(&key)
    }
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

/// Builds a stable, path-independent signature from the selection: the sorted,
/// de-duplicated, lowercased extensions of the selected paths. Two selections of
/// the same file type(s) share a signature regardless of their concrete paths.
fn type_signature(selected_paths: &[String]) -> String {
    let mut extensions: Vec<String> = selected_paths
        .iter()
        .map(|path| extension_of(path))
        .collect();
    extensions.sort();
    extensions.dedup();
    extensions.join("|")
}

/// Extracts the lowercased extension of a path's final component, or an empty
/// string when there is none. Leading-dot names (e.g. `.gitignore`) and paths
/// with no dot are treated as extensionless.
fn extension_of(path: &str) -> String {
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);

    match file_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            extension.to_ascii_lowercase()
        }
        _ => String::new(),
    }
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

/// Rebinds the path-bound `request` of a cached invocation to the live request,
/// preserving the structure-bound parts (`command_path`, `clsid`, `packaged`).
/// Path-independent invocations (the test `Fake` provider) are cloned verbatim.
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
