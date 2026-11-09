use crate::persist::PersistenceState;
use crate::size::{IndexScope, SizeService};

use crate::ipc::types::{AppConfig, SaveConfigRequest, SaveSessionRequest, SessionState};
use tauri::State;

#[tauri::command]
pub fn load_config(state: State<'_, PersistenceState>) -> AppConfig {
    state.config.current()
}

#[tauri::command(async)]
pub fn save_config(
    payload: SaveConfigRequest,
    state: State<'_, PersistenceState>,
    sizes: State<'_, SizeService>,
) -> AppConfig {
    let saved = AppConfig {
        index_exclusions: state.config.current().index_exclusions,
        ..payload.config
    };
    state.config.replace(saved.clone());
    sizes.set_index_scope(IndexScope::from_toggles(
        saved.index_folders,
        saved.index_files,
    ));
    saved
}

#[tauri::command]
pub fn load_session(state: State<'_, PersistenceState>) -> SessionState {
    state.session.current()
}

#[tauri::command]
pub fn save_session(
    payload: SaveSessionRequest,
    state: State<'_, PersistenceState>,
) -> SessionState {
    state.session.replace(payload.session.clone());
    payload.session
}
