use crate::common;

use std::sync::Arc;

use file_explorer_lib::ipc::types::{
    InvokeNativeMenuRequest, LoadNativeMenuRequest, NativeMenuIconKind, NativeMenuTargetKind,
};
use file_explorer_lib::native_menu::fake_provider::FakeNativeMenuProvider;
use file_explorer_lib::native_menu::provider::{
    dedupe_provider_items, NativeMenuProvider, ProviderInvocation, ProviderNativeMenuItem,
};
use file_explorer_lib::native_menu::token_store::{IssuedInvocation, NativeMenuTokenStore};
use file_explorer_lib::native_menu::types::NativeMenuCanonicalActionKind;
use file_explorer_lib::native_menu::unsupported::{self, UnsupportedNativeMenuProvider};
use file_explorer_lib::native_menu::NativeMenuService;

fn fixture_folder_request(request_id: &str) -> LoadNativeMenuRequest {
    load_request(
        request_id,
        MenuTarget {
            kind: NativeMenuTargetKind::Folder,
            target_path: Some("C:\\fixture"),
            folder_path: Some("C:\\fixture"),
            selected_paths: &["C:\\fixture"],
        },
    )
}

fn top_level_token(response: &file_explorer_lib::ipc::types::LoadNativeMenuResponse) -> String {
    response.items[2]
        .invoke_token
        .clone()
        .expect("top-level token")
}

#[test]
fn fake_provider_covers_backend_dedupe_and_submenus() {
    let service = NativeMenuService::new(Arc::new(FakeNativeMenuProvider));
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let response = service.load_menu(fixture_folder_request("req-provider"));

    assert_eq!(response.items.len(), 3);
    assert_eq!(response.items[0].id, "fixture-target-folder");
    assert_eq!(
        response.items[0].normalized_verb.as_deref(),
        Some("fixturefolder")
    );
    assert!(response.items[0].icon.is_none());
    assert_eq!(response.items[1].id, "fixture-archive-tools");
    assert_eq!(
        response.items[1].children[0].normalized_verb.as_deref(),
        Some("sharewithteam")
    );
    assert_eq!(response.items[1].children.len(), 1);
    assert_eq!(
        response.items[2].normalized_verb.as_deref(),
        Some("openinfixtureterminal")
    );
    let terminal_icon = response.items[2].icon.as_ref().expect("fixture icon");
    assert_eq!(terminal_icon.kind, NativeMenuIconKind::DataUrl);
    assert_eq!(terminal_icon.data_url, "data:image/png;base64,RkFLRQ==");
    assert_eq!(terminal_icon.alt.as_deref(), Some("Fixture icon"));
}

#[test]
fn fake_provider_tokens_invoke_through_the_executor_and_go_stale_on_reload() {
    let service = NativeMenuService::new(Arc::new(FakeNativeMenuProvider));
    let first_token = top_level_token(&service.load_menu(fixture_folder_request("req-provider")));
    assert!(service.execution_count() >= 1);

    let first_invoked = service.invoke_menu_action(InvokeNativeMenuRequest {
        token: first_token.clone(),
    });
    assert!(first_invoked.handled);
    assert_eq!(
        first_invoked.message.as_deref(),
        Some("invoked:fake.openTerminal")
    );

    let second_response = service.load_menu(fixture_folder_request("req-provider"));
    let second_token = top_level_token(&second_response);
    assert_ne!(first_token, second_token);

    let stale = service.invoke_menu_action(InvokeNativeMenuRequest { token: first_token });
    assert!(!stale.handled);
    assert_eq!(stale.message.as_deref(), Some("stale-or-unknown-token"));

    let child_token = second_response.items[1].children[0]
        .invoke_token
        .clone()
        .expect("child token");
    let invoked = service.invoke_menu_action(InvokeNativeMenuRequest { token: child_token });
    assert!(invoked.handled);
    assert_eq!(
        invoked.message.as_deref(),
        Some("invoked:fake.shareWithTeam")
    );
    assert!(service.execution_count() >= 3);

    let third_response = service.load_menu(fixture_folder_request("req-provider-next"));
    assert_eq!(third_response.items.len(), 3);

    let stale_after_different_request = service.invoke_menu_action(InvokeNativeMenuRequest {
        token: second_token,
    });
    assert!(!stale_after_different_request.handled);
    assert_eq!(
        stale_after_different_request.message.as_deref(),
        Some("stale-or-unknown-token")
    );
}

struct TargetMappingCase {
    kind: NativeMenuTargetKind,
    target_path: Option<&'static str>,
    folder_path: &'static str,
    selected_paths: &'static [&'static str],
    expected_first_id: &'static str,
}

const TARGET_MAPPING_CASES: [TargetMappingCase; 7] = [
    TargetMappingCase {
        kind: NativeMenuTargetKind::File,
        target_path: Some("C:\\fixture\\report.txt"),
        folder_path: "C:\\fixture",
        selected_paths: &["C:\\fixture\\report.txt"],
        expected_first_id: "fixture-target-file",
    },
    TargetMappingCase {
        kind: NativeMenuTargetKind::Folder,
        target_path: Some("C:\\fixture"),
        folder_path: "C:\\fixture",
        selected_paths: &["C:\\fixture"],
        expected_first_id: "fixture-target-folder",
    },
    TargetMappingCase {
        kind: NativeMenuTargetKind::Multi,
        target_path: Some("C:\\fixture\\report.txt"),
        folder_path: "C:\\fixture",
        selected_paths: &["C:\\fixture\\report.txt", "C:\\fixture\\notes.txt"],
        expected_first_id: "fixture-target-multi",
    },
    TargetMappingCase {
        kind: NativeMenuTargetKind::Mixed,
        target_path: Some("C:\\fixture\\report.txt"),
        folder_path: "C:\\fixture",
        selected_paths: &["C:\\fixture\\report.txt", "C:\\fixture\\docs"],
        expected_first_id: "fixture-target-mixed",
    },
    TargetMappingCase {
        kind: NativeMenuTargetKind::DriveRoot,
        target_path: Some("C:\\"),
        folder_path: "C:\\",
        selected_paths: &["C:\\"],
        expected_first_id: "fixture-target-drive-root",
    },
    TargetMappingCase {
        kind: NativeMenuTargetKind::Background,
        target_path: None,
        folder_path: "C:\\fixture",
        selected_paths: &[],
        expected_first_id: "fixture-target-background",
    },
    TargetMappingCase {
        kind: NativeMenuTargetKind::Tree,
        target_path: Some("C:\\fixture"),
        folder_path: "C:\\fixture",
        selected_paths: &["C:\\fixture"],
        expected_first_id: "fixture-target-tree",
    },
];

fn assert_item_deduped(
    response: &file_explorer_lib::ipc::types::LoadNativeMenuResponse,
    duplicate_id: &str,
    kind: &NativeMenuTargetKind,
) {
    assert!(
        response.items.iter().all(|item| item.id != duplicate_id),
        "{duplicate_id} should be deduped for {kind:?}"
    );
}

#[test]
fn fake_provider_covers_phase_five_target_mapping() {
    let service = NativeMenuService::new(Arc::new(FakeNativeMenuProvider));

    for (index, case) in TARGET_MAPPING_CASES.iter().enumerate() {
        let response = service.load_menu(load_request(
            &format!("req-target-{index}"),
            MenuTarget {
                kind: case.kind.clone(),
                target_path: case.target_path,
                folder_path: Some(case.folder_path),
                selected_paths: case.selected_paths,
            },
        ));

        assert_eq!(
            response.items.first().map(|item| item.id.as_str()),
            Some(case.expected_first_id)
        );
        assert_item_deduped(&response, "fixture-copy-duplicate", &case.kind);
        assert_item_deduped(&response, "fixture-properties-duplicate", &case.kind);
    }
}

#[test]
fn fake_provider_returns_an_empty_menu_for_tab_targets() {
    let provider = FakeNativeMenuProvider;
    let executor = file_explorer_lib::native_menu::shell_executor::ShellExecutor::default();

    let items = provider.load_menu(
        &load_request(
            "req-tab",
            MenuTarget {
                kind: NativeMenuTargetKind::Tab,
                target_path: None,
                folder_path: Some("C:\\fixture"),
                selected_paths: &[],
            },
        ),
        &executor,
    );

    assert!(items.is_empty());
}

#[test]
fn token_store_replaces_takes_and_clears_request_tokens() {
    let store = NativeMenuTokenStore::default();
    let first = store.issue_token(
        "req-a",
        ProviderInvocation::Fake {
            action_id: "first".to_string(),
        },
    );
    let second = store.issue_token(
        "req-a",
        ProviderInvocation::Fake {
            action_id: "second".to_string(),
        },
    );
    let other = store.issue_token(
        "req-b",
        ProviderInvocation::Fake {
            action_id: "other".to_string(),
        },
    );

    store.replace_request("req-a");
    assert!(store.take(&first).is_none());
    assert!(store.take(&second).is_none());
    assert_eq!(store.take(&other).expect("other token").request_id, "req-b");

    let final_token = store.issue_token(
        "req-c",
        ProviderInvocation::Fake {
            action_id: "final".to_string(),
        },
    );
    store.insert_expired_for_tests(
        "native:req-expired:stale",
        IssuedInvocation {
            request_id: "req-expired",
            invocation: ProviderInvocation::Fake {
                action_id: "stale".to_string(),
            },
        },
    );
    assert!(store.take("native:req-expired:stale").is_none());
    store.clear_all();
    assert!(store.take(&final_token).is_none());
}

#[test]
fn fake_provider_rejects_windows_shell_invocations_in_test_utils() {
    let provider = FakeNativeMenuProvider;
    let executor = file_explorer_lib::native_menu::shell_executor::ShellExecutor::default();

    let status = provider.invoke(
        &ProviderInvocation::Windows {
            request: load_request(
                "req-windows",
                MenuTarget {
                    kind: NativeMenuTargetKind::File,
                    target_path: Some("C:\\fixture\\report.txt"),
                    folder_path: Some("C:\\fixture"),
                    selected_paths: &["C:\\fixture\\report.txt"],
                },
            ),
            command_path: vec![0, 1],
        },
        &executor,
    );

    assert!(!status.handled);
    assert_eq!(
        status.message.as_deref(),
        Some("fake-provider-does-not-run-windows-shell")
    );
}

#[test]
fn fake_provider_rejects_modern_windows_invocations_in_test_utils() {
    let provider = FakeNativeMenuProvider;
    let executor = file_explorer_lib::native_menu::shell_executor::ShellExecutor::default();

    let status = provider.invoke(
        &ProviderInvocation::WindowsModern {
            clsid: 0xB41DB860_64E4_11D2_9906_E49FADC173CA,
            packaged: true,
            request: load_request(
                "req-modern",
                MenuTarget {
                    kind: NativeMenuTargetKind::File,
                    target_path: Some("C:\\fixture\\report.txt"),
                    folder_path: Some("C:\\fixture"),
                    selected_paths: &["C:\\fixture\\report.txt"],
                },
            ),
            command_path: vec![0],
        },
        &executor,
    );

    assert!(!status.handled);
    assert_eq!(
        status.message.as_deref(),
        Some("fake-provider-does-not-run-windows-shell")
    );
}

#[test]
fn unsupported_provider_returns_safe_statuses() {
    let provider = UnsupportedNativeMenuProvider;
    let executor = file_explorer_lib::native_menu::shell_executor::ShellExecutor::default();

    let items = provider.load_menu(
        &load_request(
            "req-unsupported",
            MenuTarget {
                kind: NativeMenuTargetKind::File,
                target_path: Some("C:\\fixture\\report.txt"),
                folder_path: Some("C:\\fixture"),
                selected_paths: &["C:\\fixture\\report.txt"],
            },
        ),
        &executor,
    );
    assert!(items.is_empty());

    let invoke_status = provider.invoke(
        &ProviderInvocation::Fake {
            action_id: "fake.unsupported".to_string(),
        },
        &executor,
    );
    assert!(!invoke_status.handled);
    assert_eq!(invoke_status.message.as_deref(), Some("unsupported"));

    let properties = unsupported::show_properties(&["C:\\fixture\\report.txt".to_string()]);
    assert!(!properties.handled);
    assert_eq!(properties.message.as_deref(), Some("unsupported"));

    let open_with = unsupported::open_with("C:\\fixture\\report.txt");
    assert!(!open_with.handled);
    assert_eq!(open_with.message.as_deref(), Some("unsupported"));
}

#[test]
fn provider_dedupe_preserves_submenu_containers_that_hold_non_duplicate_children() {
    let items = dedupe_provider_items(vec![ProviderNativeMenuItem {
        id: "new-submenu".to_string(),
        label: "New".to_string(),
        enabled: true,
        danger: false,
        canonical_action_kind: Some(NativeMenuCanonicalActionKind::NewFolder),
        normalized_verb: Some("new".to_string()),
        icon: None,
        invocation: None,
        children: vec![ProviderNativeMenuItem {
            id: "new-text-document".to_string(),
            label: "Text Document".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: Some("newtextdocument".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.newTextDocument".to_string(),
            }),
            children: Vec::new(),
        }],
    }]);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "New");
    assert_eq!(items[0].children.len(), 1);
    assert_eq!(items[0].children[0].label, "Text Document");
}

#[test]
fn provider_dedupe_drops_open_with_submenus_so_the_app_owned_action_takes_priority() {
    let items = dedupe_provider_items(vec![ProviderNativeMenuItem {
        id: "open-with-submenu".to_string(),
        label: "Open with".to_string(),
        enabled: true,
        danger: false,
        canonical_action_kind: Some(NativeMenuCanonicalActionKind::OpenWith),
        normalized_verb: Some("openwith".to_string()),
        icon: None,
        invocation: None,
        children: vec![ProviderNativeMenuItem {
            id: "open-with-code".to_string(),
            label: "Visual Studio Code".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: Some("openwithcode".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.openWithCode".to_string(),
            }),
            children: Vec::new(),
        }],
    }]);

    assert!(items.is_empty());
}

#[test]
fn provider_dedupe_drops_open_with_label_variants_without_needing_a_canonical_tag() {
    let items = dedupe_provider_items(vec![ProviderNativeMenuItem {
        id: "open-with-code".to_string(),
        label: "Open with Code".to_string(),
        enabled: true,
        danger: false,
        canonical_action_kind: None,
        normalized_verb: None,
        icon: None,
        invocation: Some(ProviderInvocation::Fake {
            action_id: "fake.openWithCode".to_string(),
        }),
        children: Vec::new(),
    }]);

    assert!(items.is_empty());
}

#[test]
fn provider_dedupe_omits_duplicate_native_siblings_before_renderer_guardrails() {
    let items = dedupe_provider_items(vec![
        ProviderNativeMenuItem {
            id: "first-terminal".to_string(),
            label: "Open in Terminal".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: Some("openinterminal".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.firstTerminal".to_string(),
            }),
            children: Vec::new(),
        },
        ProviderNativeMenuItem {
            id: "second-terminal".to_string(),
            label: "Open in Terminal".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: Some("openinterminal".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.secondTerminal".to_string(),
            }),
            children: Vec::new(),
        },
    ]);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "first-terminal");
}

#[test]
fn provider_dedupe_collapses_same_label_command_from_classic_and_modern_paths() {
    let items = dedupe_provider_items(vec![
        ProviderNativeMenuItem {
            id: "classic-notepadpp".to_string(),
            label: "Edit with Notepad++".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            // Classic path reports the shell verb string.
            normalized_verb: Some("nppshellverb".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.classicNotepadpp".to_string(),
            }),
            children: Vec::new(),
        },
        ProviderNativeMenuItem {
            id: "modern-notepadpp".to_string(),
            label: "Edit with Notepad++".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            // Modern path derives its verb from the label.
            normalized_verb: Some("editwithnotepad".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.modernNotepadpp".to_string(),
            }),
            children: Vec::new(),
        },
    ]);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "classic-notepadpp");
}

#[test]
fn provider_dedupe_preserves_depth_limited_submenu_parents_without_children_or_invocation() {
    let items = dedupe_provider_items(vec![ProviderNativeMenuItem {
        id: "depth-limited-submenu".to_string(),
        label: "Deep submenu".to_string(),
        enabled: true,
        danger: false,
        canonical_action_kind: None,
        normalized_verb: None,
        icon: None,
        invocation: None,
        children: Vec::new(),
    }]);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Deep submenu");
    assert!(items[0].children.is_empty());
    assert!(items[0].invocation.is_none());
}

#[test]
fn provider_dedupe_drops_always_excluded_windows_commands() {
    let items = dedupe_provider_items(vec![
        ProviderNativeMenuItem {
            id: "copy-to-folder".to_string(),
            label: "Copy to folder...".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: None,
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.copyToFolder".to_string(),
            }),
            children: Vec::new(),
        },
        ProviderNativeMenuItem {
            id: "move-to-folder".to_string(),
            label: "Move to folder...".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: None,
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.moveToFolder".to_string(),
            }),
            children: Vec::new(),
        },
        ProviderNativeMenuItem {
            id: "pin-to-start".to_string(),
            label: "Pin to Start".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: Some("pintostart".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.pinToStart".to_string(),
            }),
            children: Vec::new(),
        },
        ProviderNativeMenuItem {
            id: "include-in-library".to_string(),
            label: "Include in library".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: Some("includeinlibrary".to_string()),
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.includeInLibrary".to_string(),
            }),
            children: Vec::new(),
        },
    ]);

    assert!(items.is_empty());
}

#[test]
fn provider_dedupe_drops_always_excluded_windows_submenus() {
    let items = dedupe_provider_items(vec![ProviderNativeMenuItem {
        id: "send-to".to_string(),
        label: "Send to".to_string(),
        enabled: true,
        danger: false,
        canonical_action_kind: None,
        normalized_verb: Some("sendto".to_string()),
        icon: None,
        invocation: None,
        children: vec![ProviderNativeMenuItem {
            id: "send-to-desktop".to_string(),
            label: "Desktop (create shortcut)".to_string(),
            enabled: true,
            danger: false,
            canonical_action_kind: None,
            normalized_verb: None,
            icon: None,
            invocation: Some(ProviderInvocation::Fake {
                action_id: "fake.sendToDesktop".to_string(),
            }),
            children: Vec::new(),
        }],
    }]);

    assert!(items.is_empty());
}

struct MenuTarget<'target> {
    kind: NativeMenuTargetKind,
    target_path: Option<&'target str>,
    folder_path: Option<&'target str>,
    selected_paths: &'target [&'target str],
}

fn load_request(request_id: &str, target: MenuTarget<'_>) -> LoadNativeMenuRequest {
    LoadNativeMenuRequest {
        request_id: request_id.to_string(),
        target_kind: target.kind,
        target_path: target.target_path.map(str::to_string),
        folder_path: target.folder_path.map(str::to_string),
        selected_paths: target
            .selected_paths
            .iter()
            .map(|path| (*path).to_string())
            .collect(),
    }
}
