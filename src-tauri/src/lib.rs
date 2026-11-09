#![warn(clippy::too_many_lines)]

pub mod app_picker;
pub mod bounded_cache;
mod clipboard;
pub mod directory_session;
pub mod epoch;
pub mod error_text;
pub mod file_icons;
pub mod fs;
pub mod ipc;
pub mod item_counts;
pub mod launch;
pub mod logging;
pub mod native_drag;
pub mod native_menu;
pub mod ops;
pub mod path_text;
pub mod persist;
pub mod pixels;
pub mod reconcile;
pub mod resource_coordinator;
pub mod search;
pub mod size;
pub mod thumbnails;
pub mod trash;
pub mod traversal;
pub mod volumes;
pub mod watch;
pub mod win32;
pub mod window_chrome;

use std::path::{Path, PathBuf};

pub fn resolved_app_config_dir(base: &Path) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        match base.file_name().and_then(|name| name.to_str()) {
            Some(name) => base.with_file_name(format!("{name}-dev")),
            None => base.to_path_buf(),
        }
    }
    #[cfg(not(debug_assertions))]
    {
        base.to_path_buf()
    }
}

pub fn allow_webview_navigation(url: &tauri::Url) -> bool {
    url.scheme() != "file"
}

#[cfg(not(feature = "test-utils"))]
mod app;
#[cfg(not(feature = "test-utils"))]
pub use app::run;

#[cfg(feature = "test-utils")]
pub fn run() {}
