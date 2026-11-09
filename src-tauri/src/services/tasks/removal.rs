//! Task removal shared by the command layer and the progress surfaces.
//!
//! Deleting keeps downloaded files unless a deletion mode asks for them;
//! the progress surface's Stop button uses the same policy with files kept.

use super::TaskService;
use crate::database::Database;
use crate::error::AppError;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) fn is_terminal_download_status(status: &str) -> bool {
    matches!(status, "complete" | "error" | "removed")
}

pub(crate) async fn remove_engine_task(client: &TaskService, gid: &str) -> Result<(), AppError> {
    let operation = async {
        let mut removal_requested = false;
        loop {
            let tasks = client.tell_task_snapshot(true).await?;
            let Some(task) = tasks.iter().find(|task| task.gid == gid) else {
                return Ok(());
            };
            if is_terminal_download_status(&task.status) {
                match client.remove_download_result(gid).await {
                    Ok(_) => return Ok(()),
                    // Native result publication can trail a terminal snapshot.
                    // Re-read state on application faults; transport failures stay visible.
                    Err(AppError::Rpc { code: 1, .. }) => {}
                    Err(error) => return Err(error),
                }
            } else if !removal_requested {
                client.force_remove(gid).await?;
                removal_requested = true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), operation)
        .await
        .map_err(|_| AppError::Aria2(format!("Task {gid} did not finish removal")))?
}

pub(crate) async fn delete_task(
    client: &TaskService,
    history: &Database,
    gid: &str,
    delete_mode: Option<crate::commands::fs::FileDeletionMode>,
    protected_roots: &HashSet<PathBuf>,
) -> Result<(), AppError> {
    use crate::services::tasks::files::{
        content_paths, delete_content, history_task, path_identity, with_unfinished_copies,
    };
    let _mutation = client.mutation.lock().await;
    log::info!("aria2:delete gid={gid}");
    let snapshot = client.tell_task_snapshot(true).await?;
    let records = history.get_records(None).await?;
    let task = snapshot
        .iter()
        .find(|task| task.gid == gid)
        .cloned()
        .or_else(|| {
            records
                .iter()
                .find(|record| record.gid == gid)
                .map(history_task)
        });
    let paths = task
        .as_ref()
        .map(|task| with_unfinished_copies(content_paths(&task.files, false)))
        .unwrap_or_default();
    let root = task
        .as_ref()
        .map(|task| PathBuf::from(&task.dir))
        .unwrap_or_default();
    let mut protected_directories: HashSet<_> = snapshot
        .iter()
        .filter(|task| task.gid != gid)
        .map(|task| path_identity(Path::new(&task.dir)))
        .chain(
            records
                .iter()
                .filter(|record| record.gid != gid)
                .filter_map(|record| record.dir.as_deref())
                .map(|dir| path_identity(Path::new(dir))),
        )
        .collect();
    let protected_tasks = snapshot
        .iter()
        .chain(
            records
                .iter()
                .filter(|record| record.gid != gid)
                .map(history_task)
                .collect::<Vec<_>>()
                .iter(),
        )
        .filter(|task| task.gid != gid)
        .flat_map(|task| content_paths(&task.files, false))
        .collect::<Vec<_>>();
    protected_directories.extend(protected_roots.iter().cloned());
    protected_directories.extend(
        protected_tasks
            .iter()
            .filter_map(|path| path.parent())
            .flat_map(Path::ancestors)
            .map(path_identity),
    );
    if delete_mode.is_some() && !records.iter().any(|record| record.gid == gid) {
        if let Some(task) = &task {
            let event = crate::services::monitor::TaskEvent::from_aria2(task);
            let mut record = crate::services::monitor::build_history_record_with_added_at(
                &event,
                crate::services::monitor::events::TASK_ERROR,
                history.get_task_birth(gid).await?,
            );
            record.status = "removed".into();
            history.add_record(&record).await?;
        }
    }
    remove_engine_task(client, gid).await?;
    client.tasks.mark_deleted(gid);
    history.remove_bt_selection(gid).await?;
    if let Some(mode) = delete_mode {
        tokio::task::spawn_blocking(move || {
            let protected = protected_tasks
                .iter()
                .map(|path| path_identity(path))
                .collect();
            delete_content(&paths, &protected, &root, &protected_directories, mode)
        })
        .await
        .map_err(|error| AppError::Io(error.to_string()))??;
    }
    history.remove_task_records(gid).await
}
