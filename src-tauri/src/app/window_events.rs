use std::sync::Arc;

use tauri::{AppHandle, Manager, Window, WindowEvent};

use crate::directory_session::DirectorySessionService;
use crate::ipc::commands::{session_patch_emitter, watch_error_emitter};
use crate::reconcile::{ReconcileCoordinator, ReconcileResource};
use crate::resource_coordinator::ResourceCoordinator;
use crate::size::SizeService;
use crate::volumes::registry::VolumeRegistry;
use crate::watch::{DirPatch, WatchEmitters, WatchService};

const VOLUME_REGISTRY_RECONCILE: &str = "volume-registry";
const WATCH_SERVICE_RECONCILE: &str = "watch-service";

pub(super) fn on_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::Focused(focused) = event else {
        return;
    };
    notify_size_index(window.app_handle(), *focused);
    if *focused {
        reconcile_after_focus(window.app_handle());
    }
}

fn notify_size_index(app: &AppHandle, focused: bool) {
    if let Some(sizes) = app.try_state::<SizeService>() {
        sizes.window_focus_changed(focused);
    }
}

fn reconcile_after_focus(app: &AppHandle) {
    let reconcile = app.try_state::<Arc<ReconcileCoordinator>>();
    let coordinator = app.try_state::<Arc<ResourceCoordinator>>();
    let (Some(reconcile), Some(coordinator)) = (reconcile, coordinator) else {
        return;
    };
    let reconcile = Arc::clone(&reconcile);
    let coordinator = Arc::clone(&coordinator);
    reconcile_volume_registry(app, &reconcile, &coordinator);
    reconcile_watches(app, &reconcile, coordinator);
}

fn reconcile_volume_registry(
    app: &AppHandle,
    reconcile: &Arc<ReconcileCoordinator>,
    coordinator: &Arc<ResourceCoordinator>,
) {
    let Some(registry) = app.try_state::<Arc<VolumeRegistry>>() else {
        return;
    };
    let registry = Arc::clone(&registry);
    reconcile.request_reconcile(
        ReconcileResource {
            coordinator: Arc::clone(coordinator),
            resource_key: VOLUME_REGISTRY_RECONCILE.to_string(),
        },
        move |_generation| {
            registry.refresh();
        },
    );
}

fn reconcile_watches(
    app: &AppHandle,
    reconcile: &Arc<ReconcileCoordinator>,
    coordinator: Arc<ResourceCoordinator>,
) {
    let Some(watch) = app.try_state::<Arc<WatchService>>() else {
        return;
    };
    let watch = Arc::clone(&watch);
    let app = app.clone();
    let session_state = app
        .try_state::<Arc<DirectorySessionService>>()
        .map(|state| Arc::clone(&state));
    let reconcile_for_check = Arc::clone(reconcile);
    reconcile.request_reconcile(
        ReconcileResource {
            coordinator,
            resource_key: WATCH_SERVICE_RECONCILE.to_string(),
        },
        move |generation| {
            if !reconcile_for_check.is_current(WATCH_SERVICE_RECONCILE, generation) {
                return;
            }
            watch.reconcile(WatchEmitters {
                patch: reconcile_patch_emitter(&app, session_state.clone()),
                error: watch_error_emitter(app.clone(), "watch reconcile error"),
            });
        },
    );
}

fn reconcile_patch_emitter(
    app: &AppHandle,
    session_state: Option<Arc<DirectorySessionService>>,
) -> Arc<dyn Fn(DirPatch) + Send + Sync> {
    match session_state {
        Some(session_state) => session_patch_emitter(app.clone(), session_state),
        None => Arc::new(|_patch| {}),
    }
}
