use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use tauri::{App, Manager};

use super::emitters::frontend_emitter;
#[cfg(target_os = "macos")]
use super::emitters::size_state_emitter;
use crate::directory_session::DirectorySessionService;
use crate::ipc::events;
use crate::item_counts::ItemCountService;
use crate::native_menu::NativeMenuService;
use crate::ops::OpsService;
use crate::persist::PersistenceState;
use crate::reconcile::ReconcileCoordinator;
use crate::resource_coordinator::ResourceCoordinator;
use crate::search::service::{SearchBackend, SearchService};
use crate::size::{self, SizeService};
use crate::volumes::registry::VolumeRegistry;
use crate::watch::WatchService;
use crate::{fs, ipc, logging, resolved_app_config_dir, thumbnails, window_chrome};

const LOG_DIRECTORY: &str = "logs";

pub(super) fn setup(app: &mut App) -> Result<(), Box<dyn Error>> {
    if let Some(window) = app.get_webview_window("main") {
        window_chrome::apply_backdrop(&window);
    }
    let config_dir = resolved_app_config_dir(&app.path().app_config_dir()?);
    let persistence = PersistenceState::load(&config_dir).map_err(boxed_error)?;
    manage_logging(app, &config_dir, &persistence)?;
    app.manage(persistence);
    manage_listing_services(app);

    let resource_coordinator = Arc::new(ResourceCoordinator::new());
    app.manage(Arc::clone(&resource_coordinator));
    manage_coordinated_services(app, &resource_coordinator);
    manage_sizes_and_search(app, &resource_coordinator)?;
    manage_operations(app, resource_coordinator);
    Ok(())
}

fn boxed_error(error: impl Error + 'static) -> Box<dyn Error> {
    Box::new(error)
}

fn manage_logging(
    app: &mut App,
    config_dir: &Path,
    persistence: &PersistenceState,
) -> Result<(), Box<dyn Error>> {
    let log_dir = config_dir.join(LOG_DIRECTORY);
    let initial_level = logging::LogLevel::parse(&persistence.config.current().log_level)
        .unwrap_or(logging::LogLevel::Info);
    let logger = logging::FileLogger::new(&log_dir, initial_level).map_err(boxed_error)?;
    logging::logger::install_global(Arc::clone(&logger), initial_level);
    logging::panic::install_panic_logging(Arc::clone(&logger));
    app.manage(Arc::new(logging::LoggingState {
        dir: log_dir,
        logger,
    }));
    Ok(())
}

fn manage_listing_services(app: &mut App) {
    app.manage(Arc::new(NativeMenuService::default()));
    app.manage(ItemCountService::default());
    app.manage(Arc::new(WatchService::default()));
    app.manage(Arc::new(DirectorySessionService::default()));
    let volume_registry = Arc::new(VolumeRegistry::default());
    volume_registry.start(app.handle().clone());
    app.manage(volume_registry);
}

fn manage_coordinated_services(app: &mut App, resource_coordinator: &Arc<ResourceCoordinator>) {
    let thumbnails = Arc::new(thumbnails::ThumbnailService::new(Arc::clone(
        resource_coordinator,
    )));
    thumbnails.set_emitter(frontend_emitter(app.handle(), events::THUMBNAIL_STATE));
    app.manage(thumbnails);
    app.manage(Arc::new(ipc::executor::IpcExecutor::new(Arc::clone(
        resource_coordinator,
    ))));
    app.manage(Arc::new(ReconcileCoordinator::default()));
}

fn manage_sizes_and_search(
    app: &mut App,
    resource_coordinator: &Arc<ResourceCoordinator>,
) -> Result<(), Box<dyn Error>> {
    let sizes = size_service(app, resource_coordinator)?;
    app.manage(Arc::new(SearchService::new(
        SearchBackend::from_sizes(&sizes),
        Some(fs::default_start_dir().to_string_lossy().into_owned()),
    )));
    app.manage(sizes);
    Ok(())
}

#[cfg(target_os = "macos")]
fn size_service(
    app: &App,
    resource_coordinator: &Arc<ResourceCoordinator>,
) -> Result<SizeService, Box<dyn Error>> {
    let cache_dir = resolved_app_config_dir(&app.path().app_cache_dir()?);
    let config = app.state::<PersistenceState>().config.current();
    let host_setup = size::index::IndexHostSetup {
        options_for_start: index_options_factory(
            app.handle().clone(),
            cache_dir,
            Arc::clone(resource_coordinator),
        ),
        emit_status: frontend_emitter(app.handle(), events::SIZE_INDEX_STATUS),
        scope: size::IndexScope::from_toggles(config.index_folders, config.index_files),
    };
    Ok(SizeService::with_folder_size_index(
        size::DEFAULT_MANUAL_SIZE_TIMEOUT,
        Arc::clone(resource_coordinator),
        host_setup,
    ))
}

#[cfg(target_os = "macos")]
fn index_options_factory(
    handle: tauri::AppHandle,
    cache_dir: std::path::PathBuf,
    resource_coordinator: Arc<ResourceCoordinator>,
) -> size::index::IndexOptionsFactory {
    Box::new(move || {
        let index_setup = size::index::MacIndexSetup {
            cache_dir: &cache_dir,
            user_exclusions: persisted_index_exclusions(&handle),
            emit_sizes: size_state_emitter(&handle),
            emit_status: frontend_emitter(&handle, events::SIZE_INDEX_STATUS),
        };
        size::index::IndexOptions::for_this_mac(index_setup, Arc::clone(&resource_coordinator))
    })
}

#[cfg(target_os = "macos")]
fn persisted_index_exclusions(handle: &tauri::AppHandle) -> Vec<std::path::PathBuf> {
    handle
        .state::<PersistenceState>()
        .config
        .current()
        .index_exclusions
        .iter()
        .map(std::path::PathBuf::from)
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn size_service(
    _app: &App,
    resource_coordinator: &Arc<ResourceCoordinator>,
) -> Result<SizeService, Box<dyn Error>> {
    Ok(SizeService::with_resource_coordinator(
        size::DEFAULT_MANUAL_SIZE_TIMEOUT,
        Arc::clone(resource_coordinator),
    ))
}

fn manage_operations(app: &mut App, resource_coordinator: Arc<ResourceCoordinator>) {
    let ops = OpsService::with_resource_coordinator(resource_coordinator);
    ops.set_progress_emitter(frontend_emitter(app.handle(), events::QUEUE_PROGRESS));
    ops.set_conflict_emitter(frontend_emitter(app.handle(), events::QUEUE_CONFLICT));
    ops.set_removed_emitter(frontend_emitter(app.handle(), events::QUEUE_REMOVED));
    app.manage(ops);
}
