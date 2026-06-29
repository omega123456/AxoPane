#[path = "common/mod.rs"]
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use file_explorer_lib::ipc::types::{
    InvokeNativeMenuRequest, LoadNativeMenuRequest, NativeMenuTargetKind,
};
use file_explorer_lib::native_menu::menu_cache::MenuCache;
use file_explorer_lib::native_menu::provider::{
    NativeMenuProvider, ProviderInvocation, ProviderNativeMenuItem,
};
use file_explorer_lib::native_menu::shell_executor::ShellExecutor;
use file_explorer_lib::native_menu::types::{NativeMenuIcon, NativeMenuIconKind};
use file_explorer_lib::native_menu::NativeMenuService;

/// Provider that records every load request and returns a tree exercising all
/// invocation variants (`Windows`, `WindowsModern`, `Fake`, and a `None`
/// submenu container with a nested child).
#[derive(Default)]
struct RecordingProvider {
    load_calls: AtomicUsize,
    requests: Mutex<Vec<LoadNativeMenuRequest>>,
}

impl NativeMenuProvider for RecordingProvider {
    fn load_menu(
        &self,
        request: &LoadNativeMenuRequest,
        _executor: &ShellExecutor,
    ) -> Vec<ProviderNativeMenuItem> {
        self.load_calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request.clone());

        vec![
            ProviderNativeMenuItem {
                id: "classic".to_string(),
                label: "Classic command".to_string(),
                enabled: true,
                danger: false,
                canonical_action_kind: None,
                normalized_verb: Some("classiccommand".to_string()),
                icon: Some(NativeMenuIcon {
                    kind: NativeMenuIconKind::DataUrl,
                    data_url: "data:image/png;base64,QUJD".to_string(),
                    alt: Some("icon".to_string()),
                }),
                invocation: Some(ProviderInvocation::Windows {
                    request: request.clone(),
                    command_path: vec![3],
                }),
                children: Vec::new(),
            },
            ProviderNativeMenuItem {
                id: "tools".to_string(),
                label: "Tools".to_string(),
                enabled: true,
                danger: false,
                canonical_action_kind: None,
                normalized_verb: Some("tools".to_string()),
                icon: None,
                invocation: None,
                children: vec![ProviderNativeMenuItem {
                    id: "modern".to_string(),
                    label: "Modern command".to_string(),
                    enabled: true,
                    danger: false,
                    canonical_action_kind: None,
                    normalized_verb: Some("moderncommand".to_string()),
                    icon: None,
                    invocation: Some(ProviderInvocation::WindowsModern {
                        clsid: 0x1234_5678_9abc_def0_1122_3344_5566_7788,
                        packaged: true,
                        request: request.clone(),
                        command_path: vec![0, 2],
                    }),
                    children: Vec::new(),
                }],
            },
            ProviderNativeMenuItem {
                id: "scriptable".to_string(),
                label: "Scriptable command".to_string(),
                enabled: true,
                danger: false,
                canonical_action_kind: None,
                normalized_verb: Some("scriptablecommand".to_string()),
                icon: None,
                invocation: Some(ProviderInvocation::Fake {
                    action_id: "fake.scriptable".to_string(),
                }),
                children: Vec::new(),
            },
        ]
    }

    fn invoke(
        &self,
        invocation: &ProviderInvocation,
        _executor: &ShellExecutor,
    ) -> file_explorer_lib::ipc::types::MenuActionStatus {
        match invocation {
            ProviderInvocation::Windows { request, .. } => {
                file_explorer_lib::ipc::types::MenuActionStatus::handled_with_message(format!(
                    "windows:{}",
                    request.selected_paths.join(",")
                ))
            }
            ProviderInvocation::WindowsModern { request, .. } => {
                file_explorer_lib::ipc::types::MenuActionStatus::handled_with_message(format!(
                    "modern:{}",
                    request.selected_paths.join(",")
                ))
            }
            ProviderInvocation::Fake { action_id } => {
                file_explorer_lib::ipc::types::MenuActionStatus::handled_with_message(format!(
                    "fake:{action_id}"
                ))
            }
        }
    }
}

#[test]
fn second_load_of_same_file_type_hits_cache_and_rebinds_paths() {
    assert_eq!(common::bootstrap_message(), "phase-1-common");
    let provider = Arc::new(RecordingProvider::default());
    let service = NativeMenuService::new(provider.clone());

    let first = service.load_menu(file_request("req-1", "C:\\one\\report.txt"));
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);

    // A different .txt file: same file type, so no second provider enumeration.
    let second = service.load_menu(file_request("req-2", "C:\\two\\summary.txt"));
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);

    // The structure matches; fresh tokens are issued per request.
    assert_eq!(first.items.len(), second.items.len());
    let first_token = first.items[0].invoke_token.clone().expect("first token");
    let second_token = second.items[0].invoke_token.clone().expect("second token");
    assert_ne!(first_token, second_token);
    assert_eq!(second.items[0].icon, first.items[0].icon);

    // Invoking the cache-hit token runs against the *second* request's paths,
    // proving the cached Windows invocation was rebound.
    let invoked = service.invoke_menu_action(InvokeNativeMenuRequest {
        token: second_token,
    });
    assert!(invoked.handled);
    assert_eq!(
        invoked.message.as_deref(),
        Some("windows:C:\\two\\summary.txt")
    );

    // The nested modern invocation is rebound too.
    let modern_token = second.items[1].children[0]
        .invoke_token
        .clone()
        .expect("modern token");
    let modern_invoked = service.invoke_menu_action(InvokeNativeMenuRequest {
        token: modern_token,
    });
    assert!(modern_invoked.handled);
    assert_eq!(
        modern_invoked.message.as_deref(),
        Some("modern:C:\\two\\summary.txt")
    );

    // The path-independent Fake invocation survives the round-trip unchanged.
    let fake_token = second.items[2].invoke_token.clone().expect("fake token");
    let fake_invoked = service.invoke_menu_action(InvokeNativeMenuRequest { token: fake_token });
    assert!(fake_invoked.handled);
    assert_eq!(
        fake_invoked.message.as_deref(),
        Some("fake:fake.scriptable")
    );
}

#[test]
fn different_file_types_and_target_kinds_use_separate_cache_entries() {
    let provider = Arc::new(RecordingProvider::default());
    let service = NativeMenuService::new(provider.clone());

    service.load_menu(file_request("req-txt", "C:\\one\\report.txt"));
    service.load_menu(file_request("req-png", "C:\\one\\image.png"));
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 2);

    // Same extension but different target kind: a fresh entry.
    service.load_menu(LoadNativeMenuRequest {
        request_id: "req-tree".to_string(),
        target_kind: NativeMenuTargetKind::Tree,
        target_path: Some("C:\\one\\report.txt".to_string()),
        folder_path: Some("C:\\one".to_string()),
        selected_paths: vec!["C:\\one\\report.txt".to_string()],
    });
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 3);

    // Re-loading the original type stays a hit.
    service.load_menu(file_request("req-txt-again", "C:\\elsewhere\\notes.txt"));
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 3);
}

#[test]
fn extensionless_and_multi_selection_signatures_group_consistently() {
    let provider = Arc::new(RecordingProvider::default());
    let service = NativeMenuService::new(provider.clone());

    // Extensionless and dotfile names share the empty-extension signature.
    service.load_menu(file_request("req-license", "C:\\repo\\LICENSE"));
    service.load_menu(file_request("req-dotfile", "C:\\repo\\.gitignore"));
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);

    // Multi-selection signature is order-independent (sorted + deduped).
    service.load_menu(multi_request(
        "req-multi-a",
        &["C:\\repo\\a.txt", "C:\\repo\\b.png"],
    ));
    service.load_menu(multi_request(
        "req-multi-b",
        &["C:\\other\\c.png", "C:\\other\\d.txt"],
    ));
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cache_helpers_report_population() {
    let cache = MenuCache::default();
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);

    cache.insert(&file_request("req", "C:\\one\\report.txt"), &[]);
    assert!(!cache.is_empty());
    assert_eq!(cache.len(), 1);
    assert!(cache
        .get(&file_request("req", "C:\\two\\other.txt"))
        .is_some());
    assert!(cache
        .get(&file_request("req", "C:\\two\\other.png"))
        .is_none());
}

fn file_request(request_id: &str, path: &str) -> LoadNativeMenuRequest {
    let folder = path
        .rsplit_once('\\')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default();
    LoadNativeMenuRequest {
        request_id: request_id.to_string(),
        target_kind: NativeMenuTargetKind::File,
        target_path: Some(path.to_string()),
        folder_path: Some(folder),
        selected_paths: vec![path.to_string()],
    }
}

fn multi_request(request_id: &str, paths: &[&str]) -> LoadNativeMenuRequest {
    LoadNativeMenuRequest {
        request_id: request_id.to_string(),
        target_kind: NativeMenuTargetKind::Multi,
        target_path: paths.first().map(|path| path.to_string()),
        folder_path: Some("C:\\".to_string()),
        selected_paths: paths.iter().map(|path| path.to_string()).collect(),
    }
}
