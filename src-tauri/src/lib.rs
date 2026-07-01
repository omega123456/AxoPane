pub mod app_picker;
pub mod archive;
mod clipboard;
mod file_icons;
pub mod fs;
pub mod ipc;
pub mod launch;
pub mod logging;
pub mod native_menu;
pub mod ops;
pub mod persist;
pub mod size;
pub mod trash;
pub mod volumes;
pub mod watch;

use std::path::{Path, PathBuf};

/// Directory for config/session persistence.
///
/// Debug builds use a sibling `{identifier}-dev` folder so development and
/// release installs do not read or write the same state.
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

#[cfg(not(feature = "test-utils"))]
use ipc::commands;
#[cfg(not(feature = "test-utils"))]
use native_menu::NativeMenuService;
#[cfg(not(feature = "test-utils"))]
use ops::OpsService;
#[cfg(not(feature = "test-utils"))]
use persist::PersistenceState;
#[cfg(not(feature = "test-utils"))]
use size::SizeService;
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;
#[cfg(not(feature = "test-utils"))]
use tauri::{Emitter, Manager};
#[cfg(not(feature = "test-utils"))]
use watch::WatchService;

// Disables default browser behaviours (native right-click menu, text selection,
// drag-and-drop of links/images, browser accelerator keys, reload/zoom, etc.) so
// the webview behaves like a native desktop app rather than a web page. Our own
// React `onContextMenu` handlers still fire — only the built-in webview menu is
// suppressed.
#[cfg(not(feature = "test-utils"))]
fn prevent_default_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_prevent_default::Flags;

    // Keep DevTools shortcuts working in debug builds; block everything otherwise.
    let flags = if cfg!(debug_assertions) {
        Flags::all().difference(Flags::DEV_TOOLS)
    } else {
        Flags::all()
    };

    #[cfg(target_os = "windows")]
    {
        use tauri_plugin_prevent_default::PlatformOptions;

        tauri_plugin_prevent_default::Builder::new()
            .with_flags(flags)
            .platform(
                PlatformOptions::new()
                    .general_autofill(false)
                    .password_autosave(false)
                    // WebView2 disables F12/Ctrl+Shift+I when false; keep enabled in
                    // debug so DevTools stay reachable under `tauri dev`.
                    .browser_accelerator_keys(cfg!(debug_assertions)),
            )
            .build()
    }

    #[cfg(not(target_os = "windows"))]
    {
        tauri_plugin_prevent_default::Builder::new()
            .with_flags(flags)
            .build()
    }
}

#[cfg(not(feature = "test-utils"))]
pub fn run() {
    tauri::Builder::default()
        .plugin(prevent_default_plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let config_dir = resolved_app_config_dir(&app.path().app_config_dir()?);
            let persistence = PersistenceState::load(&config_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;

            let log_dir = config_dir.join("logs");
            let initial_level = logging::LogLevel::parse(&persistence.config.current().log_level)
                .unwrap_or(logging::LogLevel::Info);
            let logger = logging::FileLogger::new(&log_dir, initial_level)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            logging::logger::install_global(Arc::clone(&logger), initial_level);
            app.manage(logging::LoggingState {
                dir: log_dir,
                logger,
            });

            app.manage(persistence);
            app.manage(NativeMenuService::default());
            app.manage(SizeService::default());
            app.manage(WatchService::default());

            let ops = OpsService::default();
            let progress_handle = app.handle().clone();
            ops.set_progress_emitter(Arc::new(move |progress| {
                let _ = progress_handle.emit(ipc::events::QUEUE_PROGRESS, progress);
            }));
            let conflict_handle = app.handle().clone();
            ops.set_conflict_emitter(Arc::new(move |conflict| {
                let _ = conflict_handle.emit(ipc::events::QUEUE_CONFLICT, conflict);
            }));
            let removed_handle = app.handle().clone();
            ops.set_removed_emitter(Arc::new(move |operation_id| {
                let _ = removed_handle.emit(ipc::events::QUEUE_REMOVED, operation_id);
            }));
            app.manage(ops);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_initial_shell,
            commands::list_dir,
            commands::list_tree_children,
            commands::create_folder,
            commands::create_file,
            commands::rename_entry,
            commands::move_to_trash,
            commands::list_trash,
            commands::restore_from_trash,
            commands::empty_trash,
            commands::delete_from_trash,
            commands::open_path,
            commands::write_file_clipboard,
            commands::clear_file_clipboard,
            commands::load_native_menu,
            commands::invoke_native_menu_action,
            commands::show_properties,
            commands::open_with,
            commands::list_applications,
            commands::set_default_application,
            commands::get_default_application,
            commands::compress_archive,
            commands::extract_archive,
            commands::list_volumes,
            commands::everything_status,
            commands::request_folder_size,
            commands::request_folder_sizes,
            commands::cancel_size,
            commands::set_tab_watch,
            commands::refresh_tab,
            commands::load_config,
            commands::save_config,
            commands::load_session,
            commands::save_session,
            commands::start_op,
            commands::pause_op,
            commands::resume_op,
            commands::cancel_op,
            commands::reorder_ops,
            commands::resolve_conflict,
            commands::retry_op,
            commands::queue_snapshot,
            commands::has_unfinished_ops,
            commands::log_frontend,
            commands::read_logs,
            commands::set_log_level
        ])
        .run(tauri::generate_context!())
        .expect("error while running file explorer application")
}

#[cfg(feature = "test-utils")]
pub fn run() {}
