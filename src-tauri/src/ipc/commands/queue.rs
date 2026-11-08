use crate::ops::{OpSnapshot, OpsService};

use crate::ipc::types::{OpIdRequest, ReorderOpsRequest, ResolveConflictRequest, StartOpRequest};
use tauri::State;

#[tauri::command]
pub fn start_op(payload: StartOpRequest, state: State<'_, OpsService>) -> String {
    state.start_op(payload)
}

#[tauri::command]
pub fn pause_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.pause_op(&payload.id);
}

#[tauri::command]
pub fn resume_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.resume_op(&payload.id);
}

#[tauri::command]
pub fn cancel_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.cancel_op(&payload.id);
}

#[tauri::command]
pub fn skip_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.skip_op(&payload.id);
}

#[tauri::command]
pub fn retry_op(payload: OpIdRequest, state: State<'_, OpsService>) {
    state.retry_op(&payload.id);
}

#[tauri::command]
pub fn reorder_ops(payload: ReorderOpsRequest, state: State<'_, OpsService>) {
    state.reorder_ops(&payload.ids);
}

#[tauri::command]
pub fn resolve_conflict(payload: ResolveConflictRequest, state: State<'_, OpsService>) {
    state.resolve_conflict(
        &payload.id,
        payload.resolution,
        payload.apply_to_all,
        payload.rename_to,
    );
}

#[tauri::command]
pub fn queue_snapshot(state: State<'_, OpsService>) -> Vec<OpSnapshot> {
    state.snapshot()
}

#[tauri::command]
pub fn has_unfinished_ops(state: State<'_, OpsService>) -> bool {
    state.has_unfinished_work()
}
