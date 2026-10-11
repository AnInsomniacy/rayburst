use crate::engine::supervisor::{EngineOperationCause, EngineSnapshot, EngineSupervisor};
use crate::error::AppError;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn engine_supervisor_state(app: AppHandle) -> Result<EngineSnapshot, AppError> {
    let supervisor = app
        .try_state::<EngineSupervisor>()
        .ok_or_else(|| AppError::Engine("EngineSupervisor is not managed".into()))?;
    Ok(supervisor.snapshot())
}

#[tauri::command]
pub async fn engine_ensure_running(
    app: AppHandle,
    cause: EngineOperationCause,
) -> Result<EngineSnapshot, AppError> {
    let supervisor = app
        .try_state::<EngineSupervisor>()
        .ok_or_else(|| AppError::Engine("EngineSupervisor is not managed".into()))?;
    supervisor.ensure_running(&app, cause).await
}

#[tauri::command]
pub async fn engine_restart(
    app: AppHandle,
    cause: EngineOperationCause,
) -> Result<EngineSnapshot, AppError> {
    let supervisor = app
        .try_state::<EngineSupervisor>()
        .ok_or_else(|| AppError::Engine("EngineSupervisor is not managed".into()))?;
    supervisor.restart(&app, cause).await
}

#[tauri::command]
pub async fn engine_stop(
    app: AppHandle,
    cause: EngineOperationCause,
) -> Result<EngineSnapshot, AppError> {
    let supervisor = app
        .try_state::<EngineSupervisor>()
        .ok_or_else(|| AppError::Engine("EngineSupervisor is not managed".into()))?;
    supervisor.stop(&app, cause, false).await
}

#[tauri::command]
pub async fn engine_cancel(app: AppHandle) -> Result<EngineSnapshot, AppError> {
    let supervisor = app
        .try_state::<EngineSupervisor>()
        .ok_or_else(|| AppError::Engine("EngineSupervisor is not managed".into()))?;
    supervisor
        .stop(&app, EngineOperationCause::UserCancelled, false)
        .await
}

#[tauri::command]
pub async fn engine_recover_runtime_state(app: AppHandle) -> Result<EngineSnapshot, AppError> {
    let supervisor = app
        .try_state::<EngineSupervisor>()
        .ok_or_else(|| AppError::Engine("EngineSupervisor is not managed".into()))?;
    supervisor.recover_runtime_state(&app).await
}
