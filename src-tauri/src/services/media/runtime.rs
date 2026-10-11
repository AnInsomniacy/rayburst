//! Tauri lifecycle wiring and publication of confirmed download events.
use super::operation::State;
use super::{error::Error, MediaService};
use crate::services::tasks::TaskServiceState;
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Manager};
use tokio::sync::OnceCell;

pub struct MediaState(pub OnceCell<Arc<MediaService>>);
impl MediaState {
    pub fn new() -> Self {
        Self(OnceCell::new())
    }
}

pub async fn service(app: &AppHandle) -> Result<Arc<MediaService>, Error> {
    let state = app.state::<MediaState>();
    let result = state
        .0
        .get_or_try_init(|| async {
            forget_journal(app).await;
            let service = Arc::new(MediaService::new(app.state::<TaskServiceState>().0.clone()));
            let weak = Arc::downgrade(&service);
            let app = app.clone();
            tokio::spawn(async move {
                let mut maintenance_ticks = 0u32;
                loop {
                    let Some(worker) = weak.upgrade() else { break };
                    if let Err(code) = worker.maintain().await {
                        log::warn!("media: operation reconciliation failed code={code}");
                    }
                    worker.flush_events(&app).await;
                    if maintenance_ticks.is_multiple_of(720) {
                        if let Ok(tasks) = worker.engine.tell_task_snapshot(true).await {
                            let mut protected: std::collections::HashSet<_> = worker
                                .operations
                                .lock()
                                .await
                                .values()
                                .flat_map(|operation| operation.capture_ids.iter().copied())
                                .collect();
                            let mut complete = true;
                            for task in tasks {
                                if matches!(task.status.as_str(), "complete" | "removed") {
                                    continue;
                                }
                                for file in &task.files {
                                    for uri in &file.uris {
                                        if let Some(id) = super::assets::capture_id(&uri.uri) {
                                            protected.insert(id);
                                        }
                                    }
                                }
                                if task.media.is_some() {
                                    match worker.engine.get_option(&task.gid).await {
                                        Ok(options) => {
                                            if let Some(raw) = options["media-input"]
                                                .as_str()
                                                .filter(|value| !value.is_empty())
                                            {
                                                if let Ok(plan) = serde_json::from_str::<
                                                    super::contracts::InputPlan,
                                                >(
                                                    raw
                                                ) {
                                                    protected.extend(
                                                        plan.tracks
                                                            .iter()
                                                            .flat_map(|track| &track.urls)
                                                            .filter_map(|url| {
                                                                super::assets::capture_id(url)
                                                            }),
                                                    );
                                                } else {
                                                    complete = false;
                                                }
                                            }
                                        }
                                        Err(_) => complete = false,
                                    }
                                }
                            }
                            if complete {
                                if let Err(code) = super::assets::cleanup(&app, &protected).await {
                                    log::warn!("media capture cleanup failed: {code}");
                                }
                            }
                        }
                    }
                    maintenance_ticks = maintenance_ticks.wrapping_add(1);
                    drop(worker);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });
            Ok::<Arc<MediaService>, Error>(service)
        })
        .await?;
    Ok(result.clone())
}

/// Removes the operation journal earlier versions kept.
async fn forget_journal(app: &AppHandle) {
    let Ok(directory) = app.path().app_local_data_dir() else {
        return;
    };
    for suffix in ["", "-wal", "-shm"] {
        let _ =
            tokio::fs::remove_file(directory.join(format!("media-operations.db{suffix}"))).await;
    }
}

pub async fn owns(app: &AppHandle, gid: &str) -> bool {
    match app.try_state::<TaskServiceState>() {
        Some(state) => state.0.tasks.is_internal(gid).await,
        None => false,
    }
}

impl MediaService {
    pub async fn defer_event(&self, event: &'static str, task: crate::aria2::types::Aria2Task) {
        self.deferred_events
            .lock()
            .await
            .insert(task.gid.clone(), (event, task));
    }
    async fn flush_events(&self, app: &AppHandle) {
        let submitted: std::collections::HashSet<_> = self
            .operations
            .lock()
            .await
            .values()
            .filter(|op| op.state == State::Submitted)
            .map(|op| op.gid.clone())
            .collect();
        let mut pending = self.deferred_events.lock().await;
        let events: Vec<_> = submitted
            .iter()
            .filter_map(|gid| pending.remove(gid))
            .collect();
        drop(pending);
        for (event, task) in events {
            if let Err(_error) =
                super::super::monitor::process_lifecycle_task(app, event, &task, true).await
            {
                log::warn!(
                    "media: deferred lifecycle event failed code={}",
                    Error::Unavailable
                );
            }
        }
    }
}
