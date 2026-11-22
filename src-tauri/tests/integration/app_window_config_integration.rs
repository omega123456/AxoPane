use std::path::Path;

use serde_json::Value;

const BASE_CONFIG: &str = "tauri.conf.json";
const WINDOWS_OVERLAY_CONFIG: &str = "tauri.windows.conf.json";
const MAIN_WINDOW_LABEL: &str = "main";

fn configured_windows(config_file: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(config_file);
    let text = std::fs::read_to_string(&path).expect("read tauri config");
    let config: Value = serde_json::from_str(&text).expect("parse tauri config");
    config["app"]["windows"]
        .as_array()
        .expect("windows array")
        .clone()
}

fn is_main_window(window: &Value) -> bool {
    let label = window["label"].as_str().unwrap_or(MAIN_WINDOW_LABEL);
    label == MAIN_WINDOW_LABEL
}

fn assert_setup_creates_the_main_window(config_file: &str) {
    let main_window = configured_windows(config_file)
        .into_iter()
        .find(is_main_window)
        .expect("main window entry");
    assert_eq!(
        main_window["create"], false,
        "{config_file} must leave the main window for setup to create"
    );
}

#[test]
fn base_config_leaves_the_main_window_for_setup_to_create() {
    assert_setup_creates_the_main_window(BASE_CONFIG);
}

#[test]
fn windows_overlay_leaves_the_main_window_for_setup_to_create() {
    assert_setup_creates_the_main_window(WINDOWS_OVERLAY_CONFIG);
}
