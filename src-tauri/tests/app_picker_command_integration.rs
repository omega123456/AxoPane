//! Exercises the `list_applications` / `set_default_application` Tauri
//! commands under `feature = "test-utils"`. The real macOS implementation
//! (`app_picker::macos`, plutil shell-outs + LaunchServices FFI) does not
//! even compile into this test binary, so these assertions can only ever
//! observe the safe test-utils fake / unsupported fallback — never a "real
//! API success" branch — per the project's machine-global-state testing rule.

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{GetDefaultApplicationRequest, SetDefaultApplicationRequest};

#[test]
fn list_applications_returns_the_deterministic_test_utils_fake() {
    let response = commands::list_applications();

    assert_eq!(response.apps.len(), 1);
    assert_eq!(response.apps[0].name, "Fake Preview");
    assert_eq!(
        response.apps[0].bundle_path,
        "/Applications/Fake Preview.app"
    );
}

#[test]
fn set_default_application_never_writes_real_launch_services_state_under_test_utils() {
    let response = commands::set_default_application(SetDefaultApplicationRequest {
        path: "/Users/example/report.pdf".to_string(),
        bundle_path: "/Applications/Fake Preview.app".to_string(),
    });

    assert!(
        !response.handled,
        "test-utils builds must never report a real LaunchServices write as handled"
    );
    assert_eq!(response.message.as_deref(), Some("unsupported"));
}

#[test]
fn get_default_application_returns_the_deterministic_test_utils_fake() {
    let response = commands::get_default_application(GetDefaultApplicationRequest {
        path: "/Users/example/report.pdf".to_string(),
    });

    let app = response.app.expect("test-utils fake should be present");
    assert_eq!(app.name, "Fake Preview");
    assert_eq!(app.bundle_path, "/Applications/Fake Preview.app");
}
