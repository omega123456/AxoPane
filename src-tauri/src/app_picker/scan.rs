use std::fs;
use std::path::{Path, PathBuf};

pub fn scan_app_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut apps = Vec::new();
    for root in roots {
        scan_one_dir(root, &mut apps);
        scan_one_dir(&root.join("Utilities"), &mut apps);
    }
    apps.sort();
    apps.dedup();
    apps
}

fn scan_one_dir(dir: &Path, apps: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_app_bundle = path.is_dir()
            && path.extension().and_then(|extension| extension.to_str()) == Some("app");
        if is_app_bundle {
            apps.push(path);
        }
    }
}
