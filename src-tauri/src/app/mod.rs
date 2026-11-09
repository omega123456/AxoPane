mod emitters;
mod setup;
mod window_events;

use tauri::Manager;

use crate::ipc::commands;
use crate::size::SizeService;

macro_rules! registered_commands {
    () => {
        tauri::generate_handler![
            commands::get_initial_shell,
            commands::list_dir,
            commands::begin_directory_session,
            commands::get_directory_session_range,
            commands::revise_directory_session_view,
            commands::release_directory_session,
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
            commands::restart_as_admin,
            commands::open_full_disk_access_settings,
            commands::set_window_theme,
            commands::get_window_backdrop,
            commands::write_file_clipboard,
            commands::read_file_clipboard,
            commands::clear_file_clipboard,
            commands::load_native_menu,
            commands::invoke_native_menu_action,
            commands::show_properties,
            commands::open_with,
            commands::warm_native_menus,
            commands::list_applications,
            commands::set_default_application,
            commands::get_default_application,
            commands::list_volumes,
            commands::eject_volume,
            commands::folder_size_index_status,
            commands::search_query,
            commands::search_page,
            commands::search_close,
            commands::set_size_subscriptions,
            commands::request_folder_size,
            commands::request_folder_sizes,
            commands::request_icons,
            commands::request_thumbnails,
            commands::cancel_thumbnails,
            commands::request_visible_item_counts,
            commands::sort_active_items,
            commands::cancel_size,
            commands::cancel_sizes,
            commands::add_index_exclusion,
            commands::remove_index_exclusion,
            commands::pick_index_exclusions,
            commands::retry_folder_size_index,
            commands::rebuild_folder_size_index,
            commands::folder_size_index_storage,
            commands::set_tab_watch,
            commands::load_config,
            commands::save_config,
            commands::load_session,
            commands::save_session,
            commands::start_op,
            commands::pause_op,
            commands::resume_op,
            commands::cancel_op,
            commands::skip_op,
            commands::reorder_ops,
            commands::resolve_conflict,
            commands::retry_op,
            commands::queue_snapshot,
            commands::has_unfinished_ops,
            commands::log_frontend,
            commands::read_logs,
            commands::set_log_level,
            crate::native_drag::start_native_drag
        ]
    };
}

pub fn run() {
    #[cfg(target_os = "windows")]
    use_fresh_webview2_profile();

    tauri::Builder::default()
        .plugin(prevent_default_plugin())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard")
                .on_navigation(|_, url| crate::allow_webview_navigation(url))
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .on_window_event(window_events::on_window_event)
        .setup(setup::setup)
        .invoke_handler(registered_commands!())
        .build(tauri::generate_context!())
        .expect("error while building file explorer application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                flush_size_index(app);
            }
        });
}

fn flush_size_index(app: &tauri::AppHandle) {
    if let Some(sizes) = app.try_state::<SizeService>() {
        sizes.flush_index();
    }
}

#[cfg(target_os = "windows")]
fn use_fresh_webview2_profile() {
    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_some() {
        return;
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let data_dir = std::path::Path::new(&local)
        .join("com.axopane.app")
        .join("EBWebView-v2");
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", data_dir);
}

fn prevent_default_flags() -> tauri_plugin_prevent_default::Flags {
    use tauri_plugin_prevent_default::Flags;

    let blocked_everywhere = Flags::all();
    if cfg!(debug_assertions) {
        return blocked_everywhere.difference(Flags::DEV_TOOLS);
    }
    blocked_everywhere
}

fn prevent_default_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let flags = prevent_default_flags();

    #[cfg(target_os = "windows")]
    {
        use tauri_plugin_prevent_default::PlatformOptions;

        tauri_plugin_prevent_default::Builder::new()
            .with_flags(flags)
            .platform(
                PlatformOptions::new()
                    .general_autofill(false)
                    .password_autosave(false)
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
