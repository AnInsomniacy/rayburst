//! Magnet selection policy shared by every submission and window lifecycle.
use super::{files::path_identity, TaskService};
use crate::{
    aria2::types::Aria2Task,
    database::{DatabaseState, MagnetSelectionPolicy, SelectionIntent},
    error::AppError,
    services::downloads::{category, preferences},
};
use std::{collections::HashSet, path::Path};
use tauri::{AppHandle, Emitter, Manager};

pub fn needs_selection(task: &Aria2Task) -> bool {
    matches!(task.status.as_str(), "paused" | "waiting")
        && task.bittorrent.as_ref().is_some_and(|bt| {
            matches!(
                bt.file_selection_state.as_deref(),
                Some("awaiting" | "ready")
            )
        })
}

pub async fn prepare(
    app: &AppHandle,
    uris: &[String],
    options: &mut serde_json::Value,
) -> Result<(), AppError> {
    if !uris.iter().any(|uri| {
        uri.get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("magnet:"))
    }) {
        return Ok(());
    }
    let prefs = preferences::load(app)?;
    let options = options
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidInput("Download options must be an object".into()))?;
    let wait = configure(&prefs, options);
    if wait {
        let gid = options.entry("gid").or_insert_with(|| {
            uuid::Uuid::new_v4().simple().to_string()[..16]
                .to_owned()
                .into()
        });
        let gid = gid
            .as_str()
            .ok_or_else(|| AppError::InvalidInput("Invalid task ID".into()))?;
        app.state::<DatabaseState>()
            .0
            .register_bt_selection(gid, prefs.magnet_file_selection_policy)
            .await?;
    }
    Ok(())
}

fn configure(
    prefs: &preferences::Preferences,
    options: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let selected = options
        .get("select-file")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|s| !s.is_empty());
    let wait = !selected
        && (prefs.magnet_file_selection_policy != MagnetSelectionPolicy::DownloadAll
            || (prefs.file_category_enabled && !prefs.file_categories.is_empty()));
    options.insert("pause-metadata".into(), wait.to_string().into());
    options.insert("check-integrity".into(), "true".into());
    options.insert("force-save".into(), "true".into());
    wait
}

/// Rebuild presentation from durable intent and the current engine snapshot.
pub async fn decorate(app: &AppHandle, tasks: &mut [Aria2Task]) -> Result<(), AppError> {
    let database = app.state::<DatabaseState>();
    let mut intents = database.0.bt_selections().await?;
    let mut default_policy = None;
    for task in tasks {
        if app
            .state::<super::TaskServiceState>()
            .0
            .tasks
            .is_deleted(&task.gid)
        {
            continue;
        }
        if needs_selection(task) {
            let intent = match intents.get(&task.gid) {
                Some(intent) => *intent,
                None => {
                    let policy = match default_policy {
                        Some(policy) => policy,
                        None => {
                            let policy = preferences::load(app)?.magnet_file_selection_policy;
                            default_policy = Some(policy);
                            policy
                        }
                    };
                    database.0.register_bt_selection(&task.gid, policy).await?;
                    let intent = SelectionIntent {
                        policy,
                        deferred: false,
                    };
                    intents.insert(task.gid.clone(), intent);
                    intent
                }
            };
            task.selection_prompt =
                intent.policy == MagnetSelectionPolicy::Prompt && !intent.deferred;
        } else if matches!(task.status.as_str(), "complete" | "removed")
            && intents.contains_key(&task.gid)
        {
            database.0.remove_bt_selection(&task.gid).await?;
        }
    }
    Ok(())
}

pub async fn select(
    app: &AppHandle,
    engine: &TaskService,
    gid: &str,
    indices: Vec<u32>,
    automatic: bool,
) -> Result<(), AppError> {
    let _mutation = engine.mutation.lock().await;
    if engine.tasks.is_deleted(gid) {
        return Ok(());
    }
    if automatic
        && !app
            .state::<DatabaseState>()
            .0
            .bt_selections()
            .await?
            .get(gid)
            .is_some_and(|intent| {
                intent.policy == MagnetSelectionPolicy::DownloadAll && !intent.deferred
            })
    {
        return Ok(());
    }
    let task = engine.tell_status(gid).await?;
    if !needs_selection(&task) {
        return Err(AppError::InvalidInput(
            "Task is not awaiting file selection".into(),
        ));
    }
    let indices: std::collections::BTreeSet<_> = indices.into_iter().collect();
    let valid: HashSet<_> = task
        .files
        .iter()
        .filter_map(|file| file.index.parse::<u32>().ok())
        .collect();
    if indices.is_empty() || indices.iter().any(|index| !valid.contains(index)) {
        return Err(AppError::InvalidInput("Select valid torrent files".into()));
    }
    let prefs = preferences::load(app)?;
    let mut options = serde_json::json!({"select-file": indices.iter().map(u32::to_string).collect::<Vec<_>>().join(",")});
    if prefs.file_category_enabled
        && path_identity(Path::new(&task.dir)) == path_identity(Path::new(&prefs.dir))
    {
        let candidates: Vec<_> = task
            .files
            .iter()
            .filter(|file| {
                file.index
                    .parse::<u32>()
                    .is_ok_and(|index| indices.contains(&index))
                    && file.length.parse::<u64>().unwrap_or(0) > 0
            })
            .map(|file| category::Candidate {
                path: file.path.clone(),
                urls: task
                    .bittorrent
                    .as_ref()
                    .and_then(|bt| bt.magnet_link.clone())
                    .into_iter()
                    .collect(),
            })
            .collect();
        if let Some(category) = category::resolve(&candidates, &prefs.file_categories, &prefs.dir)?
        {
            options["dir"] = category.directory.into();
        }
    }
    engine.change_option(gid, options).await?;
    if task.status == "paused" {
        engine.unpause(gid).await?;
    }
    engine.save_session().await?;
    app.state::<DatabaseState>()
        .0
        .remove_bt_selection(gid)
        .await?;
    super::notify_changed(app, gid);
    Ok(())
}

/// Uses the existing monitor snapshot; no WebView or extra polling worker is needed.
pub async fn reconcile(
    app: &AppHandle,
    engine: &TaskService,
    tasks: &[Aria2Task],
) -> Result<(), AppError> {
    let mut waiting: Vec<_> = tasks
        .iter()
        .filter(|task| needs_selection(task))
        .cloned()
        .collect();
    if waiting.is_empty() {
        return Ok(());
    }
    decorate(app, &mut waiting).await?;
    let database = app.state::<DatabaseState>();
    let intents = database.0.bt_selections().await?;
    for task in waiting {
        if !intents.get(&task.gid).is_some_and(|intent| {
            intent.policy == MagnetSelectionPolicy::DownloadAll && !intent.deferred
        }) {
            continue;
        }
        let ready = task
            .bittorrent
            .as_ref()
            .and_then(|bt| bt.file_selection_state.as_deref())
            == Some("ready");
        let indices = task
            .files
            .iter()
            .filter(|file| !ready || file.selected == "true")
            .filter_map(|file| file.index.parse::<u32>().ok())
            .collect();
        if let Err(error) = select(app, engine, &task.gid, indices, true).await {
            database.0.defer_bt_selection(&task.gid, true).await?;
            log::warn!(
                "bt_selection: automatic selection failed gid={} error={error}",
                task.gid
            );
            let _ = app.emit("bt-selection:error", error.to_string());
        }
    }
    Ok(())
}

/// User pause/resume actions share the selection mutation boundary on every surface.
pub async fn pause(
    app: &AppHandle,
    engine: &TaskService,
    gid: Option<&str>,
    force: bool,
) -> Result<String, AppError> {
    let _mutation = engine.mutation.lock().await;
    let database = app.state::<DatabaseState>();
    if let Some(gid) = gid {
        database.0.defer_bt_selection(gid, true).await?;
        if force {
            engine.force_pause(gid).await
        } else {
            engine.pause(gid).await
        }
    } else {
        database.0.defer_all_bt_selections().await?;
        engine.force_pause_all().await
    }
}

pub async fn resume(app: &AppHandle, engine: &TaskService, gid: &str) -> Result<String, AppError> {
    let _mutation = engine.mutation.lock().await;
    let result = engine.unpause(gid).await?;
    app.state::<DatabaseState>()
        .0
        .defer_bt_selection(gid, false)
        .await?;
    Ok(result)
}

pub async fn resume_all(
    app: &AppHandle,
    engine: &TaskService,
) -> Result<super::ResumeEligibleResult, AppError> {
    let _mutation = engine.mutation.lock().await;
    let tasks = engine.tell_task_snapshot(false).await?;
    let result = engine.resume_eligible().await?;
    for task in tasks
        .iter()
        .filter(|task| task.status == "paused" && !needs_selection(task))
    {
        app.state::<DatabaseState>()
            .0
            .defer_bt_selection(&task.gid, false)
            .await?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_magnet_policy_covers_all_modes_classification_and_existing_selection() {
        let mut prefs = preferences::Preferences::default();
        for (policy, expected) in [
            (MagnetSelectionPolicy::Prompt, true),
            (MagnetSelectionPolicy::Manual, true),
            (MagnetSelectionPolicy::DownloadAll, false),
        ] {
            prefs.magnet_file_selection_policy = policy;
            let mut options = serde_json::Map::new();
            assert_eq!(configure(&prefs, &mut options), expected);
            assert_eq!(options["pause-metadata"], expected.to_string());
            options.insert("select-file".into(), "2".into());
            assert!(!configure(&prefs, &mut options));
            assert_eq!(options["select-file"], "2");
        }
        prefs.file_category_enabled = true;
        prefs.file_categories = serde_json::from_value(serde_json::json!([{
            "label":"Video", "directory":"Video", "directoryMode":"relative", "extensions":["mp4"]
        }]))
        .unwrap();
        assert!(configure(&prefs, &mut serde_json::Map::new()));
    }
}
