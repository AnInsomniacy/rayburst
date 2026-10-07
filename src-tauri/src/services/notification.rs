//! Native task notifications for background lifecycle events.

use super::config::RuntimeConfig;
use super::monitor::{events, TaskEvent};
use crate::error::AppError;
use crate::i18n::{
    bt_complete, download_complete, download_failed, download_start, ed2k_complete,
    resolve_preferred_locale,
};
use tauri::Manager;

/// Consume only newly submitted tasks. Restored tasks and internal probes never
/// enter this set; native progress determines when selection has actually ended.
pub async fn notify_started_tasks(
    app: &tauri::AppHandle,
    engine: &super::tasks::TaskService,
    tasks: &[crate::aria2::types::Aria2Task],
) {
    let mut names = Vec::new();
    for task in tasks {
        if matches!(task.status.as_str(), "error" | "removed") {
            engine.tasks.take_start(&task.gid).await;
            continue;
        }
        if !matches!(task.status.as_str(), "active" | "complete")
            || super::monitor::is_metadata_task(task)
            || engine.tasks.is_internal(&task.gid).await
            || task.bittorrent.as_ref().is_some_and(|bt| {
                bt.info.is_none()
                    || bt
                        .file_selection_state
                        .as_deref()
                        .is_some_and(|state| state != "none")
            })
            || task.media.as_ref().is_some_and(|media| {
                !matches!(
                    media.state.as_str(),
                    "downloading" | "recording" | "finalizing" | "complete"
                )
            })
        {
            continue;
        }
        if engine.tasks.take_start(&task.gid).await {
            names.push(TaskEvent::from_aria2(task).name);
        }
    }
    if names.is_empty() {
        return;
    }
    let config = app
        .state::<super::config::RuntimeConfigState>()
        .snapshot()
        .await;
    if let Err(error) = send_task_start_notification_from_names(app, &names, &config).await {
        log::warn!("notification:start-failed error={error}");
    }
}

#[cfg(target_os = "linux")]
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "linux")]
const LINUX_NOTIFICATION_RETENTION_TTL: Duration = Duration::from_secs(120);
#[cfg(target_os = "linux")]
const LINUX_NOTIFICATION_RETENTION_LIMIT: usize = 32;

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxNotificationIdentity {
    pub app_name: &'static str,
    pub icon: &'static str,
    pub desktop_entry: &'static str,
    pub urgency: notify_rust::Urgency,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxNotificationRetention {
    pub retained: bool,
    pub id: u32,
    pub registry_size: usize,
    pub retention_limit: usize,
    pub ttl_secs: u64,
    pub pruned_expired: usize,
    pub dropped_over_limit: usize,
}

#[cfg(target_os = "linux")]
pub struct LinuxNotificationRegistry {
    retained: Mutex<VecDeque<RetainedLinuxNotification>>,
}

#[cfg(target_os = "linux")]
struct RetainedLinuxNotification {
    created_at: Instant,
    _handle: notify_rust::NotificationHandle,
}

#[cfg(target_os = "linux")]
impl LinuxNotificationRegistry {
    pub fn new() -> Self {
        Self {
            retained: Mutex::new(VecDeque::new()),
        }
    }

    pub fn retain(&self, handle: notify_rust::NotificationHandle) -> LinuxNotificationRetention {
        let id = handle.id();
        let now = Instant::now();
        let mut retained = self
            .retained
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let pruned_expired =
            prune_expired_linux_notifications(&mut retained, now, LINUX_NOTIFICATION_RETENTION_TTL);

        retained.push_back(RetainedLinuxNotification {
            created_at: now,
            _handle: handle,
        });

        let dropped_over_limit =
            trim_linux_notifications_to_limit(&mut retained, LINUX_NOTIFICATION_RETENTION_LIMIT);

        LinuxNotificationRetention {
            retained: true,
            id,
            registry_size: retained.len(),
            retention_limit: LINUX_NOTIFICATION_RETENTION_LIMIT,
            ttl_secs: LINUX_NOTIFICATION_RETENTION_TTL.as_secs(),
            pruned_expired,
            dropped_over_limit,
        }
    }
}

#[cfg(target_os = "linux")]
impl Default for LinuxNotificationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "linux")]
fn prune_expired_linux_notifications(
    retained: &mut VecDeque<RetainedLinuxNotification>,
    now: Instant,
    ttl: Duration,
) -> usize {
    let original_len = retained.len();
    retained.retain(|notification| now.duration_since(notification.created_at) < ttl);
    original_len - retained.len()
}

#[cfg(target_os = "linux")]
fn trim_linux_notifications_to_limit(
    retained: &mut VecDeque<RetainedLinuxNotification>,
    limit: usize,
) -> usize {
    let original_len = retained.len();
    while retained.len() > limit {
        retained.pop_front();
    }
    original_len - retained.len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskNotificationKind {
    Start,
    Complete,
    P2pDownloadComplete,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskNotificationContent {
    pub kind: TaskNotificationKind,
    pub title: String,
    pub body: String,
    pub locale: String,
    /// Windows clicks reveal the file, falling back to its parent through the Shell.
    #[cfg(any(windows, test))]
    pub reveal_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationDispatchResult {
    #[cfg(not(target_os = "linux"))]
    Submitted,
    #[cfg(target_os = "linux")]
    Delivered {
        id: u32,
        identity: LinuxNotificationIdentity,
        retention: LinuxNotificationRetention,
    },
}

#[cfg(target_os = "linux")]
pub fn linux_notification_identity() -> LinuxNotificationIdentity {
    LinuxNotificationIdentity {
        app_name: "rayburst",
        icon: "rayburst",
        desktop_entry: "Rayburst",
        urgency: notify_rust::Urgency::Normal,
    }
}

fn kind_for_event(event_name: &str) -> Option<TaskNotificationKind> {
    match event_name {
        events::TASK_COMPLETE => Some(TaskNotificationKind::Complete),
        events::P2P_DOWNLOAD_COMPLETE => Some(TaskNotificationKind::P2pDownloadComplete),
        events::TASK_ERROR => Some(TaskNotificationKind::Error),
        _ => None,
    }
}

fn notification_enabled(kind: TaskNotificationKind, config: &RuntimeConfig) -> bool {
    if !config.task_notification {
        return false;
    }

    match kind {
        TaskNotificationKind::Start => config.notify_on_start,
        TaskNotificationKind::Complete | TaskNotificationKind::P2pDownloadComplete => {
            config.notify_on_complete
        }
        TaskNotificationKind::Error => true,
    }
}

pub fn build_task_notification(
    event_name: &str,
    event: &TaskEvent,
    config: &RuntimeConfig,
) -> Option<TaskNotificationContent> {
    let kind = kind_for_event(event_name)?;
    if !notification_enabled(kind, config) {
        return None;
    }

    let locale = resolve_preferred_locale(&config.locale);
    let task_name = event.name.as_str();

    let message = match kind {
        TaskNotificationKind::Start => return None,
        TaskNotificationKind::Complete => download_complete(&locale, task_name),
        TaskNotificationKind::P2pDownloadComplete => {
            if event.sharing_kind == Some("ed2k") {
                ed2k_complete(&locale, task_name)
            } else {
                bt_complete(&locale, task_name)
            }
        }
        TaskNotificationKind::Error => {
            download_failed(&locale, task_name, event.error_message.as_deref())
        }
    };

    Some(TaskNotificationContent {
        kind,
        title: message.title,
        body: message.body,
        locale,
        #[cfg(any(windows, test))]
        reveal_path: matches!(
            kind,
            TaskNotificationKind::Complete | TaskNotificationKind::P2pDownloadComplete
        )
        .then(|| {
            event
                .files
                .iter()
                .find(|file| file.selected == "true")
                .or_else(|| event.files.first())
        })
        .flatten()
        .filter(|file| !file.path.is_empty())
        .map(|file| file.path.clone()),
    })
}

pub fn build_task_start_notification(
    task_names: &[String],
    config: &RuntimeConfig,
) -> Option<TaskNotificationContent> {
    if !notification_enabled(TaskNotificationKind::Start, config) {
        return None;
    }

    let first_name = task_names
        .iter()
        .map(|name| name.trim())
        .find(|name| !name.is_empty())?;
    let locale = resolve_preferred_locale(&config.locale);
    let message = download_start(&locale, first_name, task_names.len().saturating_sub(1));

    Some(TaskNotificationContent {
        kind: TaskNotificationKind::Start,
        title: message.title,
        body: message.body,
        locale,
        #[cfg(any(windows, test))]
        reveal_path: None,
    })
}

pub async fn send_task_start_notification_from_names(
    app: &tauri::AppHandle,
    task_names: &[String],
    config: &RuntimeConfig,
) -> Result<bool, AppError> {
    let Some(content) = build_task_start_notification(task_names, config) else {
        log::debug!("notification:skip reason=preference-disabled type=Start");
        return Ok(false);
    };

    send_native_notification(app, &content).await?;
    log::debug!(
        "notification:submitted type={:?} locale={} webview_alive={}",
        content.kind,
        content.locale,
        app.get_webview_window("main").is_some()
    );
    Ok(true)
}

pub async fn send_app_notification(
    app: &tauri::AppHandle,
    title: &str,
    body: &str,
) -> Result<(), AppError> {
    let title = title.trim();
    let body = body.trim();
    if title.is_empty() || body.is_empty() {
        return Ok(());
    }

    let content = TaskNotificationContent {
        kind: TaskNotificationKind::Start,
        title: title.to_string(),
        body: body.to_string(),
        locale: "frontend".to_string(),
        #[cfg(any(windows, test))]
        reveal_path: None,
    };
    send_native_notification(app, &content).await
}

pub async fn send_task_notification(
    app: &tauri::AppHandle,
    event_name: &str,
    event: &TaskEvent,
    config: &RuntimeConfig,
) {
    let Some(kind) = kind_for_event(event_name) else {
        return;
    };

    let Some(content) = build_task_notification(event_name, event, config) else {
        log::debug!(
            "notification:skip reason=preference-disabled type={kind:?} gid={}",
            event.gid
        );
        return;
    };

    log::debug!(
        "notification:send-start type={:?} gid={} locale={} title={:?}",
        content.kind,
        event.gid,
        content.locale,
        content.title
    );

    match send_platform_notification(app, &content).await {
        Ok(dispatch) => {
            let webview_alive = app.get_webview_window("main").is_some();
            log_notification_success(&content, event, dispatch, webview_alive);
        }
        Err(e) => {
            log::warn!(
                "notification:failed type={:?} gid={} locale={} error={e}",
                content.kind,
                event.gid,
                content.locale
            );
        }
    }
}

pub async fn send_native_notification(
    app: &tauri::AppHandle,
    content: &TaskNotificationContent,
) -> Result<(), AppError> {
    send_platform_notification(app, content)
        .await
        .map(|_| ())
        .map_err(AppError::Io)
}

#[cfg(target_os = "linux")]
fn log_notification_success(
    content: &TaskNotificationContent,
    event: &TaskEvent,
    dispatch: NotificationDispatchResult,
    webview_alive: bool,
) {
    match dispatch {
        NotificationDispatchResult::Delivered {
            id,
            identity,
            retention,
        } => {
            log::info!(
                "notification:delivered platform=linux id={} type={:?} gid={} locale={} webview_alive={} app_name={} icon={} desktop_entry={} urgency=normal retained=true registry_size={} retention_limit={} ttl_secs={} pruned_expired={} dropped_over_limit={}",
                id,
                content.kind,
                event.gid,
                content.locale,
                webview_alive,
                identity.app_name,
                identity.icon,
                identity.desktop_entry,
                retention.registry_size,
                retention.retention_limit,
                retention.ttl_secs,
                retention.pruned_expired,
                retention.dropped_over_limit
            );
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn log_notification_success(
    content: &TaskNotificationContent,
    event: &TaskEvent,
    dispatch: NotificationDispatchResult,
    webview_alive: bool,
) {
    match dispatch {
        NotificationDispatchResult::Submitted => {
            log::debug!(
                "notification:submitted type={:?} gid={} locale={} webview_alive={}",
                content.kind,
                event.gid,
                content.locale,
                webview_alive
            );
        }
    }
}

#[cfg(target_os = "linux")]
async fn send_platform_notification(
    app: &tauri::AppHandle,
    content: &TaskNotificationContent,
) -> Result<NotificationDispatchResult, String> {
    let identity = linux_notification_identity();
    let handle = notify_rust::Notification::new()
        .appname(identity.app_name)
        .icon(identity.icon)
        .hint(notify_rust::Hint::DesktopEntry(
            identity.desktop_entry.to_string(),
        ))
        .urgency(identity.urgency)
        .summary(&content.title)
        .body(&content.body)
        .show_async()
        .await
        .map_err(|error| error.to_string())?;
    let registry = app.state::<LinuxNotificationRegistry>();
    let retention = registry.retain(handle);

    Ok(NotificationDispatchResult::Delivered {
        id: retention.id,
        identity,
        retention,
    })
}

#[cfg(windows)]
fn reveal_download(path: String) {
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = crate::commands::fs::show_item_in_dir(path) {
            log::warn!("notification:reveal-failed error={error}");
        }
    });
}

#[cfg(windows)]
fn register_windows_notification_identity(app_id: &str, display_name: &str) -> Result<(), String> {
    static REGISTERED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
    let mut registered = REGISTERED
        .lock()
        .map_err(|error| format!("Notification identity lock poisoned: {error}"))?;
    if !*registered {
        let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey(format!("Software\\Classes\\AppUserModelId\\{app_id}"))
            .map_err(|error| error.to_string())?;
        key.set_value("DisplayName", &display_name)
            .map_err(|error| error.to_string())?;
        *registered = true;
    }
    Ok(())
}

#[cfg(windows)]
async fn send_platform_notification(
    app: &tauri::AppHandle,
    content: &TaskNotificationContent,
) -> Result<NotificationDispatchResult, String> {
    use tauri_winrt_notification::Toast;
    let app_id = &app.config().identifier;
    register_windows_notification_identity(
        app_id,
        app.config().product_name.as_deref().unwrap_or("Rayburst"),
    )?;
    let mut toast = Toast::new(app_id)
        .title(&content.title)
        .text1(&content.body);
    if let Some(path) = content.reveal_path.clone() {
        toast = toast.on_activated(move |_| {
            reveal_download(path.clone());
            Ok(())
        });
    }
    toast.show().map_err(|error| error.to_string())?;
    Ok(NotificationDispatchResult::Submitted)
}

#[cfg(target_os = "macos")]
async fn send_platform_notification(
    app: &tauri::AppHandle,
    content: &TaskNotificationContent,
) -> Result<NotificationDispatchResult, String> {
    app.notification()
        .builder()
        .title(content.title.clone())
        .body(content.body.clone())
        .show()
        .map_err(|error| error.to_string())?;
    Ok(NotificationDispatchResult::Submitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RuntimeConfig {
        RuntimeConfig {
            locale: "en-US".to_string(),
            task_notification: true,
            notify_on_complete: true,
            notify_on_start: true,
            ..RuntimeConfig::default()
        }
    }

    fn event() -> TaskEvent {
        TaskEvent {
            media: None,
            gid: "g1".to_string(),
            name: "file.zip".to_string(),
            status: "complete".to_string(),
            error_code: None,
            error_message: None,
            dir: "/tmp".to_string(),
            total_length: "1".to_string(),
            completed_length: "1".to_string(),
            info_hash: None,
            magnet_link: None,
            sharing_time: None,
            ed2k_link: None,
            ed2k_hash: None,
            is_bt: false,
            is_ed2k: false,
            sharing_kind: None,
            files: Vec::new(),
            announce_list: Vec::new(),
        }
    }

    fn file(path: &str, selected: bool) -> super::super::monitor::TaskEventFile {
        super::super::monitor::TaskEventFile {
            index: "1".into(),
            completed_length: "1".into(),
            path: path.into(),
            length: "1".into(),
            selected: selected.to_string(),
            uris: Vec::new(),
        }
    }

    #[test]
    fn completion_reveals_selected_file_for_http_bt_and_ed2k() {
        for (event_name, sharing_kind) in [
            (events::TASK_COMPLETE, None),
            (events::P2P_DOWNLOAD_COMPLETE, Some("bt")),
            (events::P2P_DOWNLOAD_COMPLETE, Some("ed2k")),
        ] {
            let mut ev = event();
            ev.sharing_kind = sharing_kind;
            ev.files = vec![
                file("/tmp/unselected.zip", false),
                file("/tmp/result.zip", true),
            ];
            let content = build_task_notification(event_name, &ev, &cfg()).unwrap();
            assert_eq!(content.reveal_path.as_deref(), Some("/tmp/result.zip"));
        }
    }

    #[test]
    fn completion_uses_first_file_when_selection_is_absent() {
        let mut ev = event();
        ev.files = vec![
            file("/tmp/first.zip", false),
            file("/tmp/second.zip", false),
        ];
        assert_eq!(
            build_task_notification(events::TASK_COMPLETE, &ev, &cfg())
                .unwrap()
                .reveal_path
                .as_deref(),
            Some("/tmp/first.zip")
        );
    }

    #[test]
    fn missing_path_does_not_create_a_reveal_action() {
        let mut ev = event();
        assert_eq!(
            build_task_notification(events::TASK_COMPLETE, &ev, &cfg())
                .unwrap()
                .reveal_path,
            None
        );
        ev.files = vec![file("", true)];
        assert_eq!(
            build_task_notification(events::TASK_COMPLETE, &ev, &cfg())
                .unwrap()
                .reveal_path,
            None
        );
    }

    #[test]
    fn errors_and_start_notifications_do_not_reveal_incomplete_files() {
        let mut ev = event();
        ev.files = vec![file("/tmp/partial.zip", true)];
        assert_eq!(
            build_task_notification(events::TASK_ERROR, &ev, &cfg())
                .unwrap()
                .reveal_path,
            None
        );
        assert_eq!(
            build_task_start_notification(&[ev.name], &cfg())
                .unwrap()
                .reveal_path,
            None
        );
    }

    #[test]
    fn skips_completion_when_complete_notifications_are_disabled() {
        let mut config = cfg();
        config.notify_on_complete = false;
        assert!(build_task_notification(events::TASK_COMPLETE, &event(), &config).is_none());
        assert!(build_task_notification(events::TASK_ERROR, &event(), &config).is_some());
    }

    #[test]
    fn skips_all_when_task_notifications_are_disabled() {
        let mut config = cfg();
        config.task_notification = false;
        assert!(build_task_notification(events::TASK_COMPLETE, &event(), &config).is_none());
        assert!(build_task_notification(events::TASK_ERROR, &event(), &config).is_none());
        assert!(build_task_start_notification(&["file.zip".to_string()], &config).is_none());
    }

    #[test]
    fn skips_start_when_start_notifications_are_disabled() {
        let mut config = cfg();
        config.notify_on_start = false;
        assert!(build_task_start_notification(&["file.zip".to_string()], &config).is_none());
    }

    #[test]
    fn skips_start_when_task_names_are_empty() {
        assert!(build_task_start_notification(&[], &cfg()).is_none());
        assert!(build_task_start_notification(&["  ".to_string()], &cfg()).is_none());
    }
}
