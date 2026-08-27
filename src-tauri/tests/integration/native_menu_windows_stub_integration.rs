//! Exercises the Windows-only `native_menu::windows` stubs that ship under
//! `feature = "test-utils"`. The real COM-backed shell integration lives behind
//! `#[cfg(not(feature = "test-utils"))]`, so under test the module compiles to
//! safe fallbacks (empty menus / `unsupported` statuses) that never touch the
//! machine-global shell. These tests assert exactly those safe fallbacks rather
//! than allowing a "real API success" branch, per the project's machine-global
//! API testing rules. The whole file only compiles on Windows because the
//! `windows` module does not exist on other targets.
#![cfg(target_os = "windows")]

use file_explorer_lib::ipc::types::{MenuActionStatus, OpenWithRequest, ShowPropertiesRequest};
use file_explorer_lib::native_menu::helper_supervisor::HelperRole;
use file_explorer_lib::native_menu::provider::{NativeMenuProvider, ProviderInvocation};
use file_explorer_lib::native_menu::shell_executor::ShellExecutor;
use file_explorer_lib::native_menu::types::{LoadNativeMenuRequest, NativeMenuTargetKind};
use file_explorer_lib::native_menu::windows::{
    open_with, show_properties, WindowsNativeMenuProvider,
};

fn sample_request() -> LoadNativeMenuRequest {
    LoadNativeMenuRequest {
        request_id: "windows-stub".to_string(),
        target_kind: NativeMenuTargetKind::File,
        target_path: Some("C:\\fixture\\file.txt".to_string()),
        folder_path: Some("C:\\fixture".to_string()),
        selected_paths: vec!["C:\\fixture\\file.txt".to_string()],
    }
}

#[test]
fn windows_provider_load_menu_is_a_safe_empty_stub_under_test_utils() {
    let provider = WindowsNativeMenuProvider;
    let executor = ShellExecutor::new();

    let items = provider.load_menu(&sample_request(), &executor);

    assert!(
        items.is_empty(),
        "the test-utils Windows provider must not enumerate real shell menus"
    );
    assert_eq!(
        executor.execution_count(),
        1,
        "load_menu routes its work through the shell executor exactly once"
    );
}

#[test]
fn windows_provider_load_menu_for_role_delegates_to_the_safe_stub() {
    let provider = WindowsNativeMenuProvider;
    let executor = ShellExecutor::new();

    let items = provider.load_menu_for_role(&sample_request(), &executor, HelperRole::Interactive);

    assert!(
        items.is_empty(),
        "the test-utils Windows provider must not enumerate real shell menus per role"
    );
    assert_eq!(
        executor.execution_count(),
        1,
        "load_menu_for_role routes through the shell executor exactly once under test-utils"
    );
}

#[test]
fn windows_provider_invoke_reports_unsupported_under_test_utils() {
    let provider = WindowsNativeMenuProvider;
    let executor = ShellExecutor::new();

    let invocation = ProviderInvocation::Windows {
        request: sample_request(),
        command_path: vec![0],
    };
    let status = provider.invoke(&invocation, &executor);

    assert!(
        !status.handled,
        "the test-utils Windows provider must never report a real invocation as handled"
    );
    assert_eq!(executor.execution_count(), 1);
}

#[test]
fn windows_show_properties_and_open_with_fall_back_to_unsupported() {
    let properties = show_properties(&ShowPropertiesRequest {
        paths: vec!["C:\\fixture\\file.txt".to_string()],
    });
    let open = open_with(&OpenWithRequest {
        path: "C:\\fixture\\file.txt".to_string(),
    });

    let unsupported = MenuActionStatus::unsupported("unsupported");
    assert_eq!(properties.handled, unsupported.handled);
    assert!(!properties.handled);
    assert!(!open.handled);
}
