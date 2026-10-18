use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::SetWindowThemeRequest;
use file_explorer_lib::window_chrome::{
    apply_backdrop, backdrop_for_build, current_backdrop, native_theme, WindowBackdrop,
    MICA_MIN_BUILD,
};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{Theme, WebviewWindow};

fn mock_window() -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("webview");
    (app, window)
}

#[test]
fn mica_needs_a_windows_11_build() {
    assert_eq!(backdrop_for_build(None), WindowBackdrop::Solid);
    assert_eq!(backdrop_for_build(Some(19_045)), WindowBackdrop::Solid);
    assert_eq!(
        backdrop_for_build(Some(MICA_MIN_BUILD - 1)),
        WindowBackdrop::Solid
    );
    assert_eq!(
        backdrop_for_build(Some(MICA_MIN_BUILD)),
        WindowBackdrop::Mica
    );
    assert_eq!(backdrop_for_build(Some(26_100)), WindowBackdrop::Mica);
}

#[test]
fn backdrop_serializes_as_lowercase_names() {
    assert_eq!(
        serde_json::to_string(&WindowBackdrop::Mica).expect("json"),
        "\"mica\""
    );
    assert_eq!(
        serde_json::to_string(&WindowBackdrop::Solid).expect("json"),
        "\"solid\""
    );
}

#[test]
fn native_theme_follows_the_os_unless_light_or_dark() {
    assert_eq!(native_theme(Some("light")), Some(Theme::Light));
    assert_eq!(native_theme(Some("dark")), Some(Theme::Dark));
    assert_eq!(native_theme(Some("system")), None);
    assert_eq!(native_theme(None), None);
}

#[cfg(feature = "test-utils")]
#[test]
fn tests_always_get_the_solid_backdrop() {
    let (_app, window) = mock_window();

    assert_eq!(apply_backdrop(&window), WindowBackdrop::Solid);
    assert_eq!(current_backdrop(), WindowBackdrop::Solid);
    assert_eq!(commands::get_window_backdrop(), WindowBackdrop::Solid);
}

#[test]
fn set_window_theme_accepts_light_dark_and_system() {
    let (_app, window) = mock_window();

    for theme in [Some("light"), Some("dark"), None] {
        commands::set_window_theme(
            window.clone(),
            SetWindowThemeRequest {
                theme: theme.map(str::to_string),
            },
        )
        .expect("set theme");
    }
}
