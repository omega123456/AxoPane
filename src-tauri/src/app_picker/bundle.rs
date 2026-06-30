use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleMeta {
    pub name: String,
    pub bundle_id: Option<String>,
    pub icon_path: Option<PathBuf>,
}

/// Derives display metadata for an app bundle from its already-parsed
/// `Info.plist` JSON (see `macos::read_info_plist_json`, which shells out to
/// `plutil -convert json` so this function never has to deal with binary vs.
/// XML plist encoding itself).
pub fn parse_bundle_metadata(info_plist_json: &Value, bundle_dir: &Path) -> BundleMeta {
    let name = info_plist_json
        .get("CFBundleDisplayName")
        .and_then(Value::as_str)
        .or_else(|| info_plist_json.get("CFBundleName").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| fallback_name(bundle_dir));

    let bundle_id = info_plist_json
        .get("CFBundleIdentifier")
        .and_then(Value::as_str)
        .map(str::to_string);

    let icon_path = info_plist_json
        .get("CFBundleIconFile")
        .and_then(Value::as_str)
        .map(|icon_file| resolve_icon_path(bundle_dir, icon_file))
        .or_else(|| first_icns_in_resources(bundle_dir));

    BundleMeta {
        name,
        bundle_id,
        icon_path,
    }
}

fn fallback_name(bundle_dir: &Path) -> String {
    bundle_dir
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

fn resolve_icon_path(bundle_dir: &Path, icon_file: &str) -> PathBuf {
    let file_name = if icon_file.ends_with(".icns") {
        icon_file.to_string()
    } else {
        format!("{icon_file}.icns")
    };
    bundle_dir
        .join("Contents")
        .join("Resources")
        .join(file_name)
}

fn first_icns_in_resources(bundle_dir: &Path) -> Option<PathBuf> {
    let resources = bundle_dir.join("Contents").join("Resources");
    let entries = fs::read_dir(resources).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|extension| extension.to_str()) == Some("icns"))
}
