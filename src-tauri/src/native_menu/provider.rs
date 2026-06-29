use super::shell_executor::ShellExecutor;
use super::types::{LoadNativeMenuRequest, NativeMenuCanonicalActionKind, NativeMenuIcon};
use crate::ipc::types::MenuActionStatus;
use std::collections::HashSet;

const APP_OWNED_CANONICAL_ACTION_KINDS: &[NativeMenuCanonicalActionKind] = &[
    NativeMenuCanonicalActionKind::Open,
    NativeMenuCanonicalActionKind::OpenWith,
    NativeMenuCanonicalActionKind::Copy,
    NativeMenuCanonicalActionKind::Cut,
    NativeMenuCanonicalActionKind::Paste,
    NativeMenuCanonicalActionKind::Rename,
    NativeMenuCanonicalActionKind::Delete,
    NativeMenuCanonicalActionKind::Properties,
    NativeMenuCanonicalActionKind::Compress,
    NativeMenuCanonicalActionKind::Extract,
    NativeMenuCanonicalActionKind::Refresh,
    NativeMenuCanonicalActionKind::NewFolder,
    NativeMenuCanonicalActionKind::NewFile,
    NativeMenuCanonicalActionKind::SelectAll,
];

const APP_OWNED_NORMALIZED_VERBS: &[&str] = &[
    "open",
    "openwith",
    "copy",
    "cut",
    "paste",
    "rename",
    "delete",
    "properties",
    "compress",
    "extract",
    "refresh",
    "newfolder",
    "newfile",
    "selectall",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderInvocation {
    Fake {
        action_id: String,
    },
    Windows {
        request: LoadNativeMenuRequest,
        command_path: Vec<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderNativeMenuItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub danger: bool,
    pub canonical_action_kind: Option<NativeMenuCanonicalActionKind>,
    pub normalized_verb: Option<String>,
    pub icon: Option<NativeMenuIcon>,
    pub invocation: Option<ProviderInvocation>,
    pub children: Vec<ProviderNativeMenuItem>,
}

pub trait NativeMenuProvider: Send + Sync {
    fn load_menu(
        &self,
        request: &LoadNativeMenuRequest,
        executor: &ShellExecutor,
    ) -> Vec<ProviderNativeMenuItem>;

    fn invoke(&self, invocation: &ProviderInvocation, executor: &ShellExecutor)
        -> MenuActionStatus;
}

pub fn dedupe_provider_items(items: Vec<ProviderNativeMenuItem>) -> Vec<ProviderNativeMenuItem> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter_map(dedupe_provider_item)
        .filter(|item| seen.insert(dedupe_key(item)))
        .collect()
}

fn dedupe_provider_item(mut item: ProviderNativeMenuItem) -> Option<ProviderNativeMenuItem> {
    item.children = dedupe_provider_items(item.children);

    if is_app_owned_duplicate(&item) {
        return None;
    }

    Some(item)
}

fn is_app_owned_duplicate(item: &ProviderNativeMenuItem) -> bool {
    let normalized_verb = item
        .normalized_verb
        .as_deref()
        .map(normalize_verb)
        .unwrap_or_else(|| normalize_verb(&item.label));
    let always_remove_submenu = item.canonical_action_kind
        == Some(NativeMenuCanonicalActionKind::OpenWith)
        || is_open_with_variant(&normalized_verb);

    if !item.children.is_empty() && !always_remove_submenu {
        return false;
    }

    item.canonical_action_kind
        .as_ref()
        .is_some_and(|kind| APP_OWNED_CANONICAL_ACTION_KINDS.contains(kind))
        || item
            .normalized_verb
            .as_deref()
            .map(normalize_verb)
            .is_some_and(|verb| is_open_with_variant(&verb) || APP_OWNED_NORMALIZED_VERBS.contains(&verb.as_str()))
        || is_open_with_variant(&normalized_verb)
        || APP_OWNED_NORMALIZED_VERBS.contains(&normalized_verb.as_str())
}

fn normalize_verb(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn dedupe_key(item: &ProviderNativeMenuItem) -> String {
    format!(
        "{}:{}:{}",
        item.canonical_action_kind
            .as_ref()
            .map(canonical_action_key)
            .unwrap_or_default(),
        item.normalized_verb
            .as_deref()
            .map(normalize_verb)
            .unwrap_or_else(|| normalize_verb(&item.label)),
        if item.children.is_empty() {
            "leaf"
        } else {
            "submenu"
        }
    )
}

fn is_open_with_variant(value: &str) -> bool {
    value.starts_with("openwith")
}

fn canonical_action_key(kind: &NativeMenuCanonicalActionKind) -> &'static str {
    match kind {
        NativeMenuCanonicalActionKind::Open => "open",
        NativeMenuCanonicalActionKind::OpenWith => "openWith",
        NativeMenuCanonicalActionKind::Copy => "copy",
        NativeMenuCanonicalActionKind::Cut => "cut",
        NativeMenuCanonicalActionKind::Paste => "paste",
        NativeMenuCanonicalActionKind::Rename => "rename",
        NativeMenuCanonicalActionKind::Delete => "delete",
        NativeMenuCanonicalActionKind::Properties => "properties",
        NativeMenuCanonicalActionKind::Share => "share",
        NativeMenuCanonicalActionKind::Compress => "compress",
        NativeMenuCanonicalActionKind::Extract => "extract",
        NativeMenuCanonicalActionKind::Refresh => "refresh",
        NativeMenuCanonicalActionKind::NewFolder => "newFolder",
        NativeMenuCanonicalActionKind::NewFile => "newFile",
        NativeMenuCanonicalActionKind::SelectAll => "selectAll",
    }
}
