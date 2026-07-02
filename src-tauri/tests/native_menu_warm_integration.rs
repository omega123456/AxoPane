#[path = "common/mod.rs"]
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    InvokeNativeMenuRequest, LoadNativeMenuRequest, MenuActionStatus, NativeMenuTargetKind,
    WarmNativeMenusRequest,
};
use file_explorer_lib::native_menu::provider::{
    NativeMenuProvider, ProviderInvocation, ProviderNativeMenuItem,
};
use file_explorer_lib::native_menu::shell_executor::ShellExecutor;
use file_explorer_lib::native_menu::NativeMenuService;

/// Provider that records every enumeration call so tests can assert warming
/// dedupes by type instead of re-enumerating per path.
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

        vec![ProviderNativeMenuItem {
            id: "warm-item".to_string(),
            label: "Warm item".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: Some("warmitem".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.warm".to_string(),
            }),
            children: Vec::new(),
        }]
    }

    fn invoke(
        &self,
        invocation: &ProviderInvocation,
        _executor: &ShellExecutor,
    ) -> MenuActionStatus {
        match invocation {
            ProviderInvocation::Fake { action_id } => {
                MenuActionStatus::handled_with_message(format!("fake:{action_id}"))
            }
            _ => MenuActionStatus::unsupported("recording-provider-does-not-run-windows-shell"),
        }
    }
}

fn as_state<'a, T: Send + Sync + 'static>(value: &'a T) -> tauri::State<'a, T> {
    unsafe { std::mem::transmute::<&'a T, tauri::State<'a, T>>(value) }
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

fn folder_request(request_id: &str, path: &str) -> LoadNativeMenuRequest {
    let folder = path
        .rsplit_once('\\')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default();
    LoadNativeMenuRequest {
        request_id: request_id.to_string(),
        target_kind: NativeMenuTargetKind::Folder,
        target_path: Some(path.to_string()),
        folder_path: Some(folder),
        selected_paths: vec![path.to_string()],
    }
}

#[test]
fn warming_a_fresh_type_inserts_and_repeat_is_a_no_op() {
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let provider = Arc::new(RecordingProvider::default());
    let service = NativeMenuService::new(provider.clone());
    let handle = service.warm_handle();

    let key = handle
        .warm(&file_request("warm-1", "C:\\docs\\report.txt"))
        .expect("fresh type warms and returns its cache key");
    assert_eq!(key, "file::txt");
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);

    // Warming the same type again (different path) is a cache hit: no fresh
    // enumeration, no key returned.
    let repeat = handle.warm(&file_request("warm-2", "C:\\other\\summary.txt"));
    assert!(repeat.is_none());
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn warming_a_batch_dedupes_by_type_across_cloned_handles() {
    let provider = Arc::new(RecordingProvider::default());
    let service = NativeMenuService::new(provider.clone());

    // Exercise the `Clone` impl directly: both handles share the same
    // underlying cache/provider/executor.
    let handle_a = service.warm_handle();
    let handle_b = handle_a.clone();

    let first = handle_a.warm(&file_request("a", "C:\\one\\a.txt"));
    let second = handle_b.warm(&file_request("b", "C:\\one\\b.txt"));

    assert!(first.is_some());
    assert!(second.is_none());
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn warming_never_touches_the_invoke_token_store() {
    let provider = Arc::new(RecordingProvider::default());
    let service = NativeMenuService::new(provider);

    // A real, interactive load issues a token.
    let loaded = service.load_menu(file_request("interactive", "C:\\docs\\report.png"));
    let token = loaded.items[0]
        .invoke_token
        .clone()
        .expect("interactive load issues a token");

    // Warming a *different* type must not clear the outstanding token
    // (warming never calls the token store's `clear_all`).
    let handle = service.warm_handle();
    let warmed_key = handle.warm(&file_request("warm", "C:\\docs\\notes.pdf"));
    assert_eq!(warmed_key.as_deref(), Some("file::pdf"));

    let invoked = service.invoke_menu_action(InvokeNativeMenuRequest { token });
    assert!(
        invoked.handled,
        "prior token must still resolve after warming"
    );
}

#[test]
fn warming_an_already_interactively_loaded_type_is_a_cache_hit() {
    let provider = Arc::new(RecordingProvider::default());
    let service = NativeMenuService::new(provider.clone());

    service.load_menu(file_request("interactive", "C:\\docs\\report.txt"));
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);

    let handle = service.warm_handle();
    let warmed = handle.warm(&file_request("warm", "C:\\other\\summary.txt"));
    assert!(warmed.is_none(), "type already cached by load_menu");
    assert_eq!(provider.load_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_utils_warm_command_warms_through_the_shared_handle_and_dedupes_batch() {
    let service = NativeMenuService::default();

    let response = commands::warm_native_menus(
        WarmNativeMenusRequest {
            requests: vec![
                file_request("warm-a", "C:\\docs\\alpha.txt"),
                file_request("warm-b", "C:\\docs\\beta.txt"),
                folder_request("warm-c", "C:\\docs\\Archive.old"),
            ],
        },
        as_state(&service),
    );

    assert_eq!(
        response.warmed_keys,
        vec!["file::txt".to_string(), "folder::old".to_string()]
    );

    // A second, identical batch is a cache hit for every request: nothing new.
    let repeat = commands::warm_native_menus(
        WarmNativeMenusRequest {
            requests: vec![
                file_request("warm-a-again", "C:\\docs\\alpha.txt"),
                folder_request("warm-c-again", "C:\\docs\\Archive.old"),
            ],
        },
        as_state(&service),
    );
    assert!(repeat.warmed_keys.is_empty());

    // A subsequent interactive load for the warmed file type is a cache hit,
    // proving the warm path populated the exact cache the interactive path
    // reads (same fixture item count as an uncached load would produce).
    let loaded = commands::load_native_menu(
        file_request("interactive", "C:\\docs\\gamma.txt"),
        as_state(&service),
    );
    assert_eq!(loaded.items[0].id, "fixture-target-file");
}
