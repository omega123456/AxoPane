pub mod app_picker;
pub mod bounded_cache;
mod clipboard;
pub mod directory_session;
pub mod file_icons;
pub mod fs;
pub mod ipc;
pub mod item_counts;
pub mod launch;
pub mod logging;
pub mod native_menu;
pub mod ops;
pub mod persist;
pub mod reconcile;
pub mod resource_coordinator;
pub mod size;
pub mod thumbnails;
pub mod trash;
pub mod traversal;
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
use item_counts::ItemCountService;
#[cfg(not(feature = "test-utils"))]
use native_menu::NativeMenuService;
#[cfg(not(feature = "test-utils"))]
use ops::OpsService;
#[cfg(not(feature = "test-utils"))]
use persist::PersistenceState;
#[cfg(not(feature = "test-utils"))]
use resource_coordinator::ResourceCoordinator;
#[cfg(not(feature = "test-utils"))]
use size::SizeService;
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;
#[cfg(not(feature = "test-utils"))]
use std::time::Duration;
#[cfg(not(feature = "test-utils"))]
use tauri::{Emitter, Manager};
#[cfg(not(feature = "test-utils"))]
use volumes::registry::VolumeRegistry;
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
    // WebView2 bakes `scrollBarStyle` into the profile it was first created with
    // and refuses to start if a later launch requests a different value (see the
    // ScrollBarStyle docs: "must be given the same value for all webviews that
    // target the same data directory"). We switched from the auto-hiding
    // `fluentOverlay` style to the always-visible default, so upgraders whose
    // existing `EBWebView` profile was created under `fluentOverlay` would hit a
    // mismatch and the window would close instantly on launch. Move them onto a
    // fresh profile folder by overriding the data directory once. Bump the suffix
    // if a profile-baked browser argument ever changes again. Windows-only:
    // WKWebView (macOS) has no such invariant.
    #[cfg(target_os = "windows")]
    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none() {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let data_dir = std::path::Path::new(&local)
                .join("com.axopane.app")
                .join("EBWebView-v2");
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", data_dir);
        }
    }

    tauri::Builder::default()
        .plugin(prevent_default_plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .on_window_event(|window, event| {
            // Reliability net for any native volume-change/watch event the OS
            // dropped while the window was unfocused: reconciling here costs
            // nothing at rest since it only runs in response to another
            // event (focus), never on a timer.
            //
            // Phase 5 / Functional Requirement 3: this callback must only
            // *submit* work and return — no volume discovery or filesystem
            // snapshot I/O may run synchronously on the Tauri event thread.
            // `ReconcileCoordinator::request_reconcile` bumps a generation,
            // submits a `Latency`-class coordinator job, and runs the actual
            // (potentially slow) comparison on a spawned thread once
            // admitted; a stale generation's result is discarded rather than
            // published, so rapid repeated focus events coalesce onto only
            // the latest one committing.
            if let tauri::WindowEvent::Focused(true) = event {
                let reconcile = window
                    .app_handle()
                    .try_state::<Arc<reconcile::ReconcileCoordinator>>();
                let coordinator = window.app_handle().try_state::<Arc<ResourceCoordinator>>();
                let (Some(reconcile), Some(coordinator)) = (reconcile, coordinator) else {
                    return;
                };
                let reconcile = Arc::clone(&reconcile);
                let coordinator = Arc::clone(&coordinator);

                if let Some(registry) = window.app_handle().try_state::<Arc<VolumeRegistry>>() {
                    let registry = Arc::clone(&registry);
                    reconcile.request_reconcile(
                        Arc::clone(&coordinator),
                        "volume-registry".to_string(),
                        move |_generation| {
                            // Background reconcile: never do volume discovery
                            // synchronously on the window-focus callback. This
                            // is the same reliability net the previous
                            // `VolumeMonitorService::reconcile` provided for
                            // any native mount/unmount event dropped while the
                            // window was unfocused, now routed through the
                            // registry's bounded-deadline `refresh`.
                            registry.refresh();
                        },
                    );
                }

                if let Some(watch) = window.app_handle().try_state::<Arc<WatchService>>() {
                    let watch = Arc::clone(&watch);
                    let patch_handle = window.app_handle().clone();
                    let error_handle = window.app_handle().clone();
                    let session_state = window
                        .app_handle()
                        .try_state::<Arc<directory_session::DirectorySessionService>>()
                        .map(|state| Arc::clone(&state));
                    let reconcile_for_check = Arc::clone(&reconcile);
                    reconcile.request_reconcile(
                        coordinator,
                        "watch-service".to_string(),
                        move |generation| {
                            // Re-checked once more immediately before
                            // publishing any event: a slower generation must
                            // never overwrite a newer one's result, even
                            // though it already passed the pre-admission
                            // check in `request_reconcile`.
                            if !reconcile_for_check.is_current("watch-service", generation) {
                                return;
                            }
                            // `request_reconcile` retains an `Fn` callback
                            // so the coalescing drain can run the latest
                            // generation after a stale pass. Clone the app
                            // handles for this invocation before moving them
                            // into the per-event emit closures.
                            let patch_handle = patch_handle.clone();
                            let error_handle = error_handle.clone();
                            let session_state = session_state.clone();
                            watch.reconcile(
                                Arc::new(move |patch| {
                                    let Some(session_state) = session_state.as_ref() else {
                                        return;
                                    };
                                    let item_counts = patch_handle.state::<ItemCountService>();
                                    if let Some(session_patch) =
                                        ipc::commands::fold_dir_patch_into_session(
                                            session_state,
                                            &item_counts,
                                            &patch,
                                        )
                                    {
                                        let _ = patch_handle
                                            .emit(ipc::events::DIR_SESSION_PATCH, session_patch);
                                    }
                                }),
                                Arc::new(move |path, message| {
                                    log::warn!("watch reconcile error for {path}: {message}");
                                    let _ = error_handle.emit(
                                        ipc::events::WATCH_ERROR,
                                        ipc::types::WatchErrorEvent { path, message },
                                    );
                                }),
                            );
                        },
                    );
                }
            }
        })
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
            app.manage(Arc::new(logging::LoggingState {
                dir: log_dir,
                logger,
            }));

            app.manage(persistence);
            app.manage(Arc::new(NativeMenuService::default()));
            app.manage(ItemCountService::default());
            app.manage(Arc::new(WatchService::default()));
            app.manage(Arc::new(
                directory_session::DirectorySessionService::default(),
            ));
            let volume_registry = Arc::new(VolumeRegistry::default());
            volume_registry.start(app.handle().clone());
            app.manage(volume_registry);

            // Phase 2 of the performance remediation plan: the coordinator
            // is registered here so it lives for the app's lifetime and is
            // available as `tauri::State<'_, Arc<ResourceCoordinator>>`.
            // Phase 5 is the first subsystem migrated onto it: window-focus
            // reconciliation (`ReconcileCoordinator`, below) routes its
            // background comparison work through this coordinator's
            // `Latency` lane instead of running synchronously on the
            // Tauri event thread. Its `Drop` impl shuts the dispatcher
            // thread down deterministically when this `Arc` is finally
            // dropped (app teardown).
            let resource_coordinator = Arc::new(ResourceCoordinator::new());
            app.manage(Arc::clone(&resource_coordinator));
            let thumbnails = Arc::new(thumbnails::ThumbnailService::new(Arc::clone(
                &resource_coordinator,
            )));
            let thumbnail_handle = app.handle().clone();
            thumbnails.set_emitter(Arc::new(move |events| {
                let _ = thumbnail_handle.emit(ipc::events::THUMBNAIL_STATE, events);
            }));
            app.manage(thumbnails);
            app.manage(Arc::new(ipc::executor::IpcExecutor::new(Arc::clone(
                &resource_coordinator,
            ))));
            app.manage(Arc::new(reconcile::ReconcileCoordinator::default()));

            // `SizeService` is constructed here (after the shared coordinator
            // exists) rather than at its own `app.manage(...)` call further
            // up, specifically so it can be handed the same shared
            // coordinator `OpsService` uses below instead of building a
            // private one — folder-size traversal is then fairly scheduled
            // against the same throughput/CPU/latency lanes as every other
            // subsystem instead of having its own independent admission
            // budget.
            app.manage(SizeService::with_resource_coordinator(
                Duration::from_secs(2),
                Arc::clone(&resource_coordinator),
            ));

            let ops = OpsService::with_resource_coordinator(resource_coordinator);
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
            commands::write_file_clipboard,
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
            commands::everything_status,
            commands::request_folder_size,
            commands::request_folder_sizes,
            commands::request_icons,
            commands::request_thumbnails,
            commands::cancel_thumbnails,
            commands::request_visible_item_counts,
            commands::sort_active_items,
            commands::cancel_size,
            commands::cancel_sizes,
            commands::set_tab_watch,
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
