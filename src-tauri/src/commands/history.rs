//! History commands backed by process-owned SQLite storage.
use crate::database::{DatabaseState, HistoryPage, HistoryPageInput, HistoryRecord};
use crate::error::AppError;
use tauri::Manager;
use tauri::State;

#[tauri::command]
pub async fn rename_completed_file(
    app: tauri::AppHandle,
    gid: String,
    name: String,
) -> Result<(), AppError> {
    crate::services::downloads::validate_filename(&name)?;
    let database = &app.state::<DatabaseState>().0;
    // Never discard the engine's terminal result before history owns it.
    if database.get_record(&gid).await?.is_none() {
        return Err(AppError::Conflict(
            "Download history is not ready yet".into(),
        ));
    }
    let engine = app.state::<crate::services::tasks::TaskServiceState>();
    let _mutation = engine.0.mutation.lock().await;
    if let Some(task) = engine
        .0
        .tell_task_snapshot(true)
        .await?
        .iter()
        .find(|task| task.gid == gid)
    {
        if task.status != "complete" || task.seeder.as_deref() == Some("true") {
            return Err(AppError::Conflict(
                "Finish the task before renaming its file".into(),
            ));
        }
        engine.0.remove_download_result(&gid).await?;
        engine.0.tasks.mark_deleted(&gid);
    }
    app.state::<DatabaseState>()
        .0
        .rename_completed_file(&gid, &name)
        .await?;
    crate::services::pump::wake(&app);
    Ok(())
}

/// Add or update a history record (upsert by GID).
#[tauri::command]
pub async fn history_add_record(
    state: State<'_, DatabaseState>,
    record: HistoryRecord,
) -> Result<(), AppError> {
    state.0.add_record(&record).await
}

/// Read an unordered history snapshot, optionally filtered by status.
#[tauri::command]
pub async fn history_get_records(
    state: State<'_, DatabaseState>,
    status: Option<String>,
) -> Result<Vec<HistoryRecord>, AppError> {
    state.0.get_records(status.as_deref()).await
}

/// Remove a single record by GID.
#[tauri::command]
pub async fn history_remove_record(
    state: State<'_, DatabaseState>,
    gid: String,
) -> Result<(), AppError> {
    state.0.remove_record(&gid).await
}

/// Clear records, optionally filtered by status.
#[tauri::command]
pub async fn history_clear_records(
    state: State<'_, DatabaseState>,
    status: Option<String>,
) -> Result<(), AppError> {
    state.0.clear_records(status.as_deref()).await
}

/// Remove records whose GIDs are in the provided list.
#[tauri::command]
pub async fn history_remove_stale(
    state: State<'_, DatabaseState>,
    gids: Vec<String>,
) -> Result<(), AppError> {
    state.0.remove_stale_records(&gids).await
}

/// Record a task birth timestamp.
#[tauri::command]
pub async fn history_record_birth(
    state: State<'_, DatabaseState>,
    gid: String,
    added_at: String,
) -> Result<(), AppError> {
    state.0.record_task_birth(&gid, &added_at).await
}

/// Load all birth records.
#[tauri::command]
pub async fn history_load_births(
    state: State<'_, DatabaseState>,
) -> Result<Vec<(String, String)>, AppError> {
    state.0.load_birth_records().await
}

/// Run PRAGMA integrity_check.
#[tauri::command]
pub async fn history_check_integrity(state: State<'_, DatabaseState>) -> Result<String, AppError> {
    state.0.check_integrity().await
}

#[tauri::command]
pub async fn history_get_record(
    state: State<'_, DatabaseState>,
    gid: String,
) -> Result<Option<HistoryRecord>, AppError> {
    state.0.get_record(&gid).await
}
#[tauri::command]
pub async fn history_get_page(
    state: State<'_, DatabaseState>,
    input: HistoryPageInput,
) -> Result<HistoryPage, AppError> {
    state.0.get_records_page(input).await
}
#[tauri::command]
pub async fn history_remove_births(
    state: State<'_, DatabaseState>,
    gids: Vec<String>,
) -> Result<(), AppError> {
    state.0.remove_birth_records(&gids).await
}
