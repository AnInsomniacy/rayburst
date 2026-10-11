//! The single reader of engine state.
//!
//! Each tick reads the global statistics and every task queue with one
//! multicall and feeds the native surfaces (tray, Dock, taskbar, keep-awake),
//! the lifecycle monitor and, while a window exists, the frontend. Engine
//! notifications and task actions request an immediate tick, so the interval
//! only bounds how stale speeds and progress may get.

use super::config::RuntimeConfigState;
use super::monitor::Monitor;
use super::stat::{StatEffects, StatUpdate};
use crate::aria2::types::{Aria2GlobalStat, Aria2Task};
use crate::error::AppError;
use crate::services::tasks::TaskService;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Notify};

/// Tick interval while the window is visible.
const VISIBLE_INTERVAL: Duration = Duration::from_millis(500);
/// Tick interval without a visible window while tasks are active.
const ACTIVE_INTERVAL: Duration = Duration::from_secs(1);
const IDLE_INTERVAL: Duration = Duration::from_secs(2);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(10);

pub const SNAPSHOT_EVENT: &str = "tasks:snapshot";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub stat: StatUpdate,
    /// Active, waiting and stopped tasks without internal ones.
    pub tasks: Vec<Aria2Task>,
}

#[derive(Default)]
struct Published {
    sequence: u64,
    snapshot: Option<Arc<Snapshot>>,
}

struct Running {
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

/// Managed for the whole app lifetime; reads run only between
/// [`Pump::start`] and [`Pump::stop`], i.e. while an engine is up.
pub struct Pump {
    wake: Notify,
    published: watch::Sender<Published>,
    running: Mutex<Option<Running>>,
    /// The task whose details are open; it keeps its whole file list.
    detailed: Mutex<Option<String>>,
}

/// Files a WebView snapshot carries per task. A torrent can hold tens of
/// thousands; serializing them every tick froze the window.
const SNAPSHOT_FILES: usize = 64;

/// Keeps the selected files first, so opening a task still finds one.
fn trim_files(task: &mut Aria2Task) {
    if task.files.len() <= SNAPSHOT_FILES {
        return;
    }
    task.files_total = Some(task.files.len());
    task.files.sort_by_key(|file| file.selected != "true");
    task.files.truncate(SNAPSHOT_FILES);
    task.files
        .sort_by_key(|file| file.index.parse::<u64>().unwrap_or(u64::MAX));
}

impl Pump {
    pub fn new() -> Self {
        Self {
            wake: Notify::new(),
            published: watch::Sender::new(Published::default()),
            running: Mutex::new(None),
            detailed: Mutex::new(None),
        }
    }

    /// Names the task whose details are open, or none.
    pub fn set_detailed(&self, gid: Option<String>) {
        *self
            .detailed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = gid;
        self.wake();
    }

    /// Requests a tick now; repeated requests before it runs coalesce.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Waits for a snapshot read after this call.
    pub async fn refresh(&self) -> Result<Arc<Snapshot>, AppError> {
        let mut published = self.published.subscribe();
        let requested = published.borrow_and_update().sequence;
        self.wake();
        let next = tokio::time::timeout(
            REFRESH_TIMEOUT,
            published.wait_for(|value| value.sequence > requested && value.snapshot.is_some()),
        )
        .await
        .map_err(|_| AppError::Engine("Download engine did not answer".into()))?
        .map_err(|_| AppError::Engine("Download engine is not running".into()))?;
        Ok(next.snapshot.clone().expect("checked by wait_for"))
    }

    pub fn start(self: &Arc<Self>, app: AppHandle, aria2: Arc<TaskService>) {
        let (stop, stopped) = watch::channel(false);
        let pump = Arc::clone(self);
        let task = tokio::spawn(async move { pump.run(app, aria2, stopped).await });
        let previous = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(Running { stop, task });
        if let Some(previous) = previous {
            let _ = previous.stop.send(true);
        }
    }

    pub async fn stop(&self) {
        let running = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(running) = running {
            let _ = running.stop.send(true);
            if let Err(error) = running.task.await {
                log::warn!("pump: join failed: {error}");
            }
        }
    }

    async fn run(
        &self,
        app: AppHandle,
        aria2: Arc<TaskService>,
        mut stopped: watch::Receiver<bool>,
    ) {
        let mut effects = StatEffects::default();
        let mut monitor: Option<Monitor> = None;
        loop {
            let window = app.get_webview_window("main");
            let visible = window
                .as_ref()
                .is_some_and(|window| window.is_visible().unwrap_or(false));
            let interval = match self
                .tick(&app, &aria2, window.is_some(), &mut effects, &mut monitor)
                .await
            {
                Ok(_) if visible => VISIBLE_INTERVAL,
                Ok(true) => ACTIVE_INTERVAL,
                Ok(false) => IDLE_INTERVAL,
                Err(error) => {
                    // A busy engine answers late; only a process exit restarts it.
                    log::debug!("pump: read failed: {error}");
                    IDLE_INTERVAL
                }
            };
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = self.wake.notified() => {}
                _ = stopped.changed() => {
                    effects.release().await;
                    log::info!("pump: stopped");
                    return;
                }
            }
        }
    }

    /// Returns whether any task is active.
    async fn tick(
        &self,
        app: &AppHandle,
        aria2: &TaskService,
        window: bool,
        effects: &mut StatEffects,
        monitor: &mut Option<Monitor>,
    ) -> Result<bool, AppError> {
        // Stopped tasks matter only to the task list.
        let (stat, mut tasks) = aria2.snapshot(window).await?;
        let stat = stat_update(&stat);

        let config = match app.try_state::<RuntimeConfigState>() {
            Some(state) => state.snapshot().await,
            None => super::config::RuntimeConfig::default(),
        };
        let active: Vec<_> = tasks
            .iter()
            .filter(|task| task.status == "active")
            .cloned()
            .collect();
        effects.apply(app, &config, &stat, &active).await;

        let queued: Vec<_> = tasks
            .iter()
            .filter(|task| matches!(task.status.as_str(), "active" | "waiting" | "paused"))
            .cloned()
            .collect();
        match monitor {
            Some(monitor) => monitor.observe(app, aria2, &queued).await,
            None => *monitor = Some(Monitor::new(&queued)),
        }

        if window {
            if let Err(error) = super::tasks::bittorrent::decorate(app, &mut tasks).await {
                log::warn!("pump: selection state unavailable: {error}");
            }
        }
        let any_active = stat.num_active > 0;
        let detailed = self
            .detailed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for task in &mut tasks {
            if detailed.as_deref() != Some(task.gid.as_str()) {
                trim_files(task);
            }
        }
        let snapshot = Arc::new(Snapshot { stat, tasks });
        if window {
            if let Err(error) = app.emit(SNAPSHOT_EVENT, &*snapshot) {
                log::debug!("pump: emit failed: {error}");
            }
        }
        self.published.send_modify(|published| {
            published.sequence += 1;
            published.snapshot = Some(snapshot);
        });
        Ok(any_active)
    }
}

fn stat_update(stat: &Aria2GlobalStat) -> StatUpdate {
    let number = |value: &str| value.parse::<u64>().unwrap_or(0);
    StatUpdate {
        download_speed: number(&stat.download_speed),
        upload_speed: number(&stat.upload_speed),
        num_active: number(&stat.num_active),
        num_waiting: number(&stat.num_waiting),
        num_stopped: number(&stat.num_stopped),
        num_stopped_total: number(&stat.num_stopped_total),
    }
}

/// Requests an immediate snapshot, e.g. after a task changed.
pub fn wake(app: &AppHandle) {
    if let Some(pump) = app.try_state::<Arc<Pump>>() {
        pump.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_file_lists_keep_selected_files_in_index_order() {
        let mut task = Aria2Task {
            files: (0..200)
                .map(|index| crate::aria2::types::Aria2File {
                    index: (index + 1).to_string(),
                    selected: if index >= 150 { "true" } else { "false" }.into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        super::trim_files(&mut task);
        assert_eq!(task.files_total, Some(200));
        assert_eq!(task.files.len(), super::SNAPSHOT_FILES);
        assert_eq!(task.files[0].index, "1");
        assert!(
            task.files
                .iter()
                .filter(|file| file.selected == "true")
                .count()
                == 50
        );
        assert!(task.files.windows(2).all(|pair| {
            pair[0].index.parse::<u64>().unwrap() < pair[1].index.parse::<u64>().unwrap()
        }));

        let mut small = Aria2Task {
            files: vec![Default::default(); 3],
            ..Default::default()
        };
        super::trim_files(&mut small);
        assert_eq!((small.files.len(), small.files_total), (3, None));
    }

    #[test]
    fn stat_strings_parse_and_garbage_reads_as_zero() {
        let stat = Aria2GlobalStat {
            download_speed: "1048576".into(),
            upload_speed: "x".into(),
            num_active: "2".into(),
            num_waiting: "1".into(),
            num_stopped: "3".into(),
            num_stopped_total: "4".into(),
        };
        let update = stat_update(&stat);
        assert_eq!(update.download_speed, 1_048_576);
        assert_eq!(update.upload_speed, 0);
        assert_eq!(update.num_active, 2);
        assert_eq!(update.num_stopped_total, 4);
    }

    #[tokio::test]
    async fn refresh_returns_the_next_published_snapshot() {
        let pump = Arc::new(Pump::new());
        let reader = Arc::clone(&pump);
        let refresh = tokio::spawn(async move { reader.refresh().await });
        tokio::task::yield_now().await;
        pump.wake.notified().await;
        pump.published.send_modify(|published| {
            published.sequence += 1;
            published.snapshot = Some(Arc::new(Snapshot {
                stat: StatUpdate::default(),
                tasks: Vec::new(),
            }));
        });
        assert!(refresh.await.unwrap().is_ok());
    }
}
