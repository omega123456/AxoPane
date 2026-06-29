pub mod archive;
mod clipboard;
mod file_icons;
pub mod fs;
pub mod ipc;
pub mod launch;
pub mod native_menu;
pub mod ops;
pub mod persist;
pub mod size;
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

#[cfg(not(feature = "test-utils"))]
fn log_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{Target, TargetKind};

    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    tauri_plugin_log::Builder::new()
        .level(level)
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::Webview),
        ])
        .build()
}

#[cfg(not(feature = "test-utils"))]
pub fn run() {
    tauri::Builder::default()
        .plugin(log_plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let config_dir = resolved_app_config_dir(&app.path().app_config_dir()?);
            let persistence = PersistenceState::load(&config_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;

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
            commands::delete_entries,
            commands::open_path,
            commands::write_file_clipboard,
            commands::clear_file_clipboard,
            commands::load_native_menu,
            commands::invoke_native_menu_action,
            commands::show_properties,
            commands::open_with,
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
            commands::log_frontend
        ])
        .run(tauri::generate_context!())
        .expect("error while running file explorer application")
}

#[cfg(feature = "test-utils")]
pub fn run() {}
