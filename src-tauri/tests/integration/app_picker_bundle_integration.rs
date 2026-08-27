//! `app_picker::bundle` parses already-decoded `Info.plist` JSON (the
//! `plutil -convert json` shell-out itself lives in the macOS-only real
//! implementation, gated out of test builds). These tests feed `serde_json`
//! fixtures directly, so they exercise the parsing/fallback logic without any
//! dependency on a real app bundle.

use file_explorer_lib::app_picker::bundle::parse_bundle_metadata;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn prefers_display_name_over_bundle_name_and_filename() {
    let bundle_dir = tempdir().expect("tempdir");
    let bundle_path = bundle_dir.path().join("Example.app");
    fs::create_dir_all(&bundle_path).expect("create bundle dir");

    let plist = json!({
        "CFBundleDisplayName": "Example Display",
        "CFBundleName": "Example Short",
        "CFBundleIdentifier": "com.example.app",
    });

    let meta = parse_bundle_metadata(&plist, &bundle_path);

    assert_eq!(meta.name, "Example Display");
    assert_eq!(meta.bundle_id.as_deref(), Some("com.example.app"));
}

#[test]
fn falls_back_to_bundle_name_when_display_name_missing() {
    let bundle_dir = tempdir().expect("tempdir");
    let bundle_path = bundle_dir.path().join("Example.app");
    fs::create_dir_all(&bundle_path).expect("create bundle dir");

    let plist = json!({ "CFBundleName": "Example Short" });

    let meta = parse_bundle_metadata(&plist, &bundle_path);

    assert_eq!(meta.name, "Example Short");
}

#[test]
fn falls_back_to_filename_when_name_keys_missing() {
    let bundle_dir = tempdir().expect("tempdir");
    let bundle_path = bundle_dir.path().join("MyApp.app");
    fs::create_dir_all(&bundle_path).expect("create bundle dir");

    let plist = json!({});

    let meta = parse_bundle_metadata(&plist, &bundle_path);

    assert_eq!(meta.name, "MyApp");
    assert_eq!(meta.bundle_id, None);
}

#[test]
fn resolves_icon_path_from_icon_file_key_appending_extension() {
    let bundle_dir = tempdir().expect("tempdir");
    let bundle_path = bundle_dir.path().join("Example.app");
    fs::create_dir_all(&bundle_path).expect("create bundle dir");

    let plist = json!({ "CFBundleIconFile": "AppIcon" });

    let meta = parse_bundle_metadata(&plist, &bundle_path);

    assert_eq!(
        meta.icon_path,
        Some(bundle_path.join("Contents/Resources/AppIcon.icns"))
    );
}

#[test]
fn icon_file_key_already_ending_in_icns_is_not_double_suffixed() {
    let bundle_dir = tempdir().expect("tempdir");
    let bundle_path = bundle_dir.path().join("Example.app");
    fs::create_dir_all(&bundle_path).expect("create bundle dir");

    let plist = json!({ "CFBundleIconFile": "AppIcon.icns" });

    let meta = parse_bundle_metadata(&plist, &bundle_path);

    assert_eq!(
        meta.icon_path,
        Some(bundle_path.join("Contents/Resources/AppIcon.icns"))
    );
}

#[test]
fn falls_back_to_first_icns_in_resources_when_icon_file_key_missing() {
    let bundle_dir = tempdir().expect("tempdir");
    let bundle_path = bundle_dir.path().join("Example.app");
    let resources = bundle_path.join("Contents/Resources");
    fs::create_dir_all(&resources).expect("create resources dir");
    fs::write(resources.join("readme.txt"), b"noise").expect("write noise file");
    fs::write(resources.join("Found.icns"), b"fake-icns-bytes").expect("write icns file");

    let plist = json!({});

    let meta = parse_bundle_metadata(&plist, &bundle_path);

    assert_eq!(meta.icon_path, Some(resources.join("Found.icns")));
}

#[test]
fn missing_icon_file_key_and_no_icns_in_resources_yields_none() {
    let bundle_dir = tempdir().expect("tempdir");
    let bundle_path = bundle_dir.path().join("Example.app");
    fs::create_dir_all(bundle_path.join("Contents/Resources")).expect("create resources dir");

    let plist = json!({});

    let meta = parse_bundle_metadata(&plist, &bundle_path);

    assert_eq!(meta.icon_path, None);
}
