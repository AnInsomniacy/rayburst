//! Per-download progress on the desktop's job surfaces.
//!
//! A system progress bar is not a portable API. KDE Plasma tracks one D-Bus
//! job view per download, Windows shows one progress toast per download, and
//! macOS only paints the aggregate Dock tile. The notification-style
//! surfaces live here; the Dock and taskbar bars stay in [`super::stat`].
//!
//! A download whose size is not known yet (no `Content-Length`, live media)
//! has no percentage and is shown with an indeterminate bar.
//!
//! While these surfaces are enabled they replace the start and completion
//! notifications for the same downloads, because they already announce them.
//! A surface pauses and resumes only its own download; stopping removes it
//! with downloaded files kept.

use super::config::RuntimeConfig;
use crate::aria2::types::Aria2Task;
use std::collections::HashSet;
#[cfg(not(any(target_os = "linux", windows)))]
use std::future::Future;

#[cfg(not(any(target_os = "macos", windows)))]
pub const BACKEND_NONE: &str = "none";
pub const BACKEND_KDE_JOB_VIEW: &str = "kde-job-view";
pub const BACKEND_WINDOWS_TOAST: &str = "windows-toast";
#[cfg(target_os = "macos")]
pub const BACKEND_MACOS_DOCK: &str = "macos-dock";

/// A backend that retries soon after a failure, e.g. while plasmashell restarts.
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(30);
/// Speeds change every tick; the surfaces need no more than one update a second.
const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
/// A stalled desktop must not hold up the pump's task snapshots.
const SURFACE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// One desktop progress surface: a single downloading task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobProgress {
    pub gid: String,
    pub title: String,
    /// Speed or other live detail; short enough for a toast.
    pub status: String,
    /// `None` while the final size is unknown; the bar is then indeterminate.
    pub percent: Option<u32>,
    pub speed: u64,
}

/// Describes every actively downloading task as its own job.
pub fn describe(active: &[Aria2Task]) -> Vec<JobProgress> {
    active
        .iter()
        .filter(|task| {
            task.seeder.as_deref() != Some("true") && !super::monitor::is_metadata_task(task)
        })
        .filter_map(|task| {
            let title = super::monitor::TaskEvent::known_name(task)?;
            let speed = task.download_speed.parse::<u64>().unwrap_or(0);
            Some(JobProgress {
                gid: task.gid.clone(),
                title,
                status: format!("{}/s", super::stat::compact_size(speed)),
                percent: task_percent(task),
                speed,
            })
        })
        .collect()
}

/// A task's percentage, or `None` when its size is still unknown.
fn task_percent(task: &Aria2Task) -> Option<u32> {
    if let Some(media) = &task.media {
        if media.live == "true" || media.state == "finalizing" {
            return None;
        }
        return media
            .progress
            .as_deref()?
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|fraction| (fraction.clamp(0.0, 1.0) * 100.0) as u32);
    }
    let total = task.total_length.parse::<u64>().ok()?;
    if total == 0 {
        return None;
    }
    let completed = task.completed_length.parse::<u64>().unwrap_or(0);
    Some(((completed as f64 / total as f64).clamp(0.0, 1.0) * 100.0) as u32)
}

/// Whether this desktop's backend shows progress as a notification surface.
pub fn shows_progress_notification() -> bool {
    matches!(backend_id(), BACKEND_KDE_JOB_VIEW | BACKEND_WINDOWS_TOAST)
}

/// Whether the progress surface replaces start and completion notifications.
pub fn notifications_replaced(config: &RuntimeConfig) -> bool {
    config.show_progress_bar && shows_progress_notification()
}

/// The backend this platform uses without asking the desktop.
#[cfg(target_os = "macos")]
pub fn backend_id() -> &'static str {
    BACKEND_MACOS_DOCK
}

#[cfg(windows)]
pub fn backend_id() -> &'static str {
    BACKEND_WINDOWS_TOAST
}

#[cfg(target_os = "linux")]
pub fn backend_id() -> &'static str {
    if plasma_session() {
        BACKEND_KDE_JOB_VIEW
    } else {
        BACKEND_NONE
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub fn backend_id() -> &'static str {
    BACKEND_NONE
}

#[cfg(target_os = "linux")]
fn plasma_session() -> bool {
    std::env::var_os("KDE_FULL_SESSION").is_some_and(|value| value != "false")
        || std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|value| {
            value
                .split(':')
                .any(|part| part.eq_ignore_ascii_case("KDE") || part.eq_ignore_ascii_case("plasma"))
        })
}

#[cfg(target_os = "linux")]
async fn pause_task(app: &tauri::AppHandle, gid: &str) -> bool {
    use tauri::Manager;

    let Some(engine) = app.try_state::<crate::services::tasks::TaskServiceState>() else {
        return false;
    };
    match engine.0.force_pause(gid).await {
        Ok(_) => {
            crate::services::pump::wake(app);
            true
        }
        Err(error) => {
            log::debug!("job_progress:pause-skipped gid={gid} error={error}");
            false
        }
    }
}

#[cfg(target_os = "linux")]
async fn resume_task(app: &tauri::AppHandle, gid: &str) {
    use tauri::Manager;

    let Some(engine) = app.try_state::<crate::services::tasks::TaskServiceState>() else {
        return;
    };
    if let Err(error) = engine.0.unpause(gid).await {
        log::debug!("job_progress:resume-skipped gid={gid} error={error}");
    }
    crate::services::pump::wake(app);
}

/// Stops one download with the usual policy, which keeps downloaded files.
async fn stop_task(app: &tauri::AppHandle, gid: &str) {
    use tauri::Manager;

    let Some(engine) = app.try_state::<crate::services::tasks::TaskServiceState>() else {
        return;
    };
    let Some(db) = app.try_state::<crate::database::DatabaseState>() else {
        log::warn!("job_progress:stop-unavailable");
        return;
    };
    let protected = std::collections::HashSet::new();
    if let Err(error) =
        crate::services::tasks::removal::delete_task(&engine.0, &db.0, gid, None, &protected).await
    {
        log::warn!("job_progress:stop-failed gid={gid} error={error}");
    } else {
        log::info!("job_progress:stopped gid={gid}");
    }
    crate::services::pump::wake(app);
}

/// Keeps one surface per downloading task and closes it when the task ends.
#[derive(Default)]
pub struct JobProgressEffect {
    surfaces: std::collections::HashMap<String, Surface>,
    retry_at: Option<std::time::Instant>,
}

impl JobProgressEffect {
    pub async fn sync(
        &mut self,
        app: &tauri::AppHandle,
        config: &RuntimeConfig,
        jobs: &[JobProgress],
        paused: &[Aria2Task],
    ) {
        if !config.show_progress_bar || !shows_progress_notification() {
            self.shutdown(app).await;
            return;
        }
        let paused: HashSet<&str> = paused.iter().map(|task| task.gid.as_str()).collect();
        let active: HashSet<&str> = jobs.iter().map(|job| job.gid.as_str()).collect();

        // A task that is neither running nor paused is done with its surface.
        let finished: Vec<String> = self
            .surfaces
            .keys()
            .filter(|gid| !active.contains(gid.as_str()) && !paused.contains(gid.as_str()))
            .cloned()
            .collect();
        for gid in finished {
            if let Some(surface) = self.surfaces.remove(&gid) {
                if tokio::time::timeout(SURFACE_TIMEOUT, surface.finish(app, false))
                    .await
                    .is_err()
                {
                    log::debug!("job_progress:finish-timeout backend={}", backend_id());
                }
            }
        }

        for gid in &paused {
            if let Some(surface) = self.surfaces.get_mut(*gid) {
                if tokio::time::timeout(SURFACE_TIMEOUT, surface.set_paused(true))
                    .await
                    .is_err()
                {
                    log::debug!("job_progress:pause-timeout backend={}", backend_id());
                }
            }
        }

        for job in jobs {
            if let Some(surface) = self.surfaces.get_mut(&job.gid) {
                if tokio::time::timeout(SURFACE_TIMEOUT, surface.set_paused(false))
                    .await
                    .is_err()
                {
                    log::debug!("job_progress:pause-timeout backend={}", backend_id());
                }
                if tokio::time::timeout(SURFACE_TIMEOUT, surface.update(job))
                    .await
                    .is_err()
                {
                    log::debug!("job_progress:update-timeout backend={}", backend_id());
                }
            }
        }

        if self
            .retry_at
            .is_some_and(|at| std::time::Instant::now() < at)
        {
            return;
        }
        for job in jobs {
            if self.surfaces.contains_key(&job.gid) {
                continue;
            }
            match tokio::time::timeout(SURFACE_TIMEOUT, begin_surface(app, config, job)).await {
                Ok(Some(surface)) => {
                    self.surfaces.insert(job.gid.clone(), surface);
                    self.retry_at = None;
                }
                Ok(None) => self.retry_at = Some(std::time::Instant::now() + RETRY_AFTER),
                Err(_) => {
                    log::warn!("job_progress:begin-timeout backend={}", backend_id());
                    self.retry_at = Some(std::time::Instant::now() + RETRY_AFTER);
                }
            }
        }
    }

    /// Closes every surface without a completion notification, for shutdown
    /// or when the preference is turned off.
    pub async fn shutdown(&mut self, app: &tauri::AppHandle) {
        let closes = self
            .surfaces
            .drain()
            .map(|(_, surface)| surface.finish(app, true));
        if tokio::time::timeout(SURFACE_TIMEOUT, futures_util::future::join_all(closes))
            .await
            .is_err()
        {
            log::debug!("job_progress:finish-timeout backend={}", backend_id());
        }
    }
}

#[cfg(any(target_os = "linux", windows))]
enum Surface {
    #[cfg(target_os = "linux")]
    Kde(KdeJob),
    #[cfg(windows)]
    Toast(WindowsToast),
}

#[cfg(any(target_os = "linux", windows))]
impl Surface {
    async fn set_paused(&mut self, paused: bool) {
        match self {
            #[cfg(target_os = "linux")]
            Surface::Kde(view) => view.set_paused(paused).await,
            #[cfg(windows)]
            Surface::Toast(_) => {
                let _ = paused;
            }
        }
    }

    async fn update(&mut self, job: &JobProgress) {
        match self {
            #[cfg(target_os = "linux")]
            Surface::Kde(view) => {
                if let Err(error) = view.update(job).await {
                    log::debug!("job_progress:kde-update-failed error={error}");
                }
            }
            #[cfg(windows)]
            Surface::Toast(toast) => toast.update(job),
        }
    }

    async fn finish(self, app: &tauri::AppHandle, silent: bool) {
        match self {
            #[cfg(target_os = "linux")]
            Surface::Kde(view) => view.finish(app, silent).await,
            #[cfg(windows)]
            Surface::Toast(toast) => {
                let _ = (app, silent);
                drop(toast);
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
struct Surface;

#[cfg(not(any(target_os = "linux", windows)))]
impl Surface {
    async fn set_paused(&mut self, _paused: bool) {}

    async fn update(&mut self, _job: &JobProgress) {}

    async fn finish(self, _app: &tauri::AppHandle, _silent: bool) {}
}

#[cfg(not(any(target_os = "linux", windows)))]
fn begin_surface(
    _app: &tauri::AppHandle,
    _config: &RuntimeConfig,
    _job: &JobProgress,
) -> impl Future<Output = Option<Surface>> {
    std::future::ready(None)
}

#[cfg(target_os = "linux")]
async fn begin_surface(
    app: &tauri::AppHandle,
    _config: &RuntimeConfig,
    job: &JobProgress,
) -> Option<Surface> {
    if backend_id() != BACKEND_KDE_JOB_VIEW {
        return None;
    }
    match KdeJob::begin(app, job).await {
        Ok(view) => {
            log::debug!("job_progress:backend=kde-job-view gid={}", job.gid);
            Some(Surface::Kde(view))
        }
        Err(error) => {
            log::warn!(
                "job_progress:kde-begin-failed gid={} error={error}",
                job.gid
            );
            None
        }
    }
}

#[cfg(windows)]
async fn begin_surface(
    app: &tauri::AppHandle,
    config: &RuntimeConfig,
    job: &JobProgress,
) -> Option<Surface> {
    match WindowsToast::begin(app, config, job) {
        Ok(toast) => {
            log::debug!("job_progress:backend=windows-toast gid={}", job.gid);
            Some(Surface::Toast(toast))
        }
        Err(error) => {
            log::warn!(
                "job_progress:toast-begin-failed gid={} error={error}",
                job.gid
            );
            None
        }
    }
}

// ── KDE Plasma job view ─────────────────────────────────────────────
//
// Each download owns one job view. The job belongs to the D-Bus connection
// that requested it, so the connection is kept for the whole job; dropping it
// removes the view. `KJob::Killable` asks for the stop button and
// `KJob::Suspendable` for the pause/resume toggle.

#[cfg(target_os = "linux")]
const KDE_JOB_VIEW_SERVICE: &str = "org.kde.JobViewServer";
#[cfg(target_os = "linux")]
const KDE_JOB_VIEW_INTERFACE: &str = "org.kde.JobViewV2";
#[cfg(target_os = "linux")]
const KDE_JOB_VIEW_CAPABILITIES: i32 = 0x1 | 0x2;
/// `KIO::ERR_USER_CANCELED` (`KJob::KilledJobError`). Plasma removes a job
/// terminated with this error instead of announcing it as finished.
#[cfg(target_os = "linux")]
const KIO_ERR_USER_CANCELED: u32 = 1;

#[cfg(target_os = "linux")]
#[zbus::proxy(
    interface = "org.kde.JobViewServer",
    default_service = "org.kde.JobViewServer",
    default_path = "/JobViewServer"
)]
trait JobViewServer {
    #[zbus(name = "requestView")]
    fn request_view(
        &self,
        app_name: &str,
        app_icon_name: &str,
        capabilities: i32,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[cfg(target_os = "linux")]
#[zbus::proxy(interface = "org.kde.JobViewV2")]
trait JobView {
    #[zbus(name = "setInfoMessage")]
    fn set_info_message(&self, message: &str) -> zbus::Result<()>;
    #[zbus(name = "setPercent")]
    fn set_percent(&self, percent: u32) -> zbus::Result<()>;
    #[zbus(name = "setSpeed")]
    fn set_speed(&self, bytes_per_second: u64) -> zbus::Result<()>;
    #[zbus(name = "setSuspended")]
    fn set_suspended(&self, suspended: bool) -> zbus::Result<()>;
    #[zbus(name = "setError")]
    fn set_error(&self, error_code: u32) -> zbus::Result<()>;
    #[zbus(name = "terminate")]
    fn terminate(&self, error_message: &str) -> zbus::Result<()>;
}

#[cfg(target_os = "linux")]
struct KdeJob {
    gid: String,
    connection: zbus::Connection,
    path: zbus::zvariant::OwnedObjectPath,
    listener: tokio::task::JoinHandle<()>,
    canceled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    suspended: bool,
    last: JobProgress,
    updated_at: std::time::Instant,
}

#[cfg(target_os = "linux")]
impl KdeJob {
    async fn begin(app: &tauri::AppHandle, job: &JobProgress) -> Result<Self, String> {
        let connection = zbus::Connection::session()
            .await
            .map_err(|error| error.to_string())?;
        let server = JobViewServerProxy::new(&connection)
            .await
            .map_err(|error| error.to_string())?;
        let path = server
            .request_view("Rayburst", "rayburst", KDE_JOB_VIEW_CAPABILITIES)
            .await
            .map_err(|error| error.to_string())?;
        let view = job_view(&connection, &path)
            .await
            .map_err(|error| error.to_string())?;
        view.set_info_message(&job.title)
            .await
            .map_err(|error| error.to_string())?;
        // Plasma draws an indeterminate bar while the percentage is below 1.
        view.set_percent(job.percent.unwrap_or(0))
            .await
            .map_err(|error| error.to_string())?;
        view.set_speed(job.speed)
            .await
            .map_err(|error| error.to_string())?;

        let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let listener = spawn_control_listener(
            app.clone(),
            connection.clone(),
            path.clone(),
            job.gid.clone(),
            canceled.clone(),
        )
        .await?;
        Ok(Self {
            gid: job.gid.clone(),
            connection,
            path,
            listener,
            canceled,
            suspended: false,
            last: job.clone(),
            updated_at: std::time::Instant::now(),
        })
    }

    async fn set_paused(&mut self, paused: bool) {
        if self.suspended == paused {
            return;
        }
        match job_view(&self.connection, &self.path).await {
            Ok(view) => {
                if let Err(error) = view.set_suspended(paused).await {
                    log::debug!("job_progress:kde-suspend-failed error={error}");
                    return;
                }
                self.suspended = paused;
            }
            Err(error) => log::debug!("job_progress:kde-suspend-failed error={error}"),
        }
    }

    async fn update(&mut self, job: &JobProgress) -> zbus::Result<()> {
        let mode_changed = self.last.percent.is_some() != job.percent.is_some();
        let percent_changed = self.last.percent != job.percent;
        if !mode_changed && !percent_changed && self.updated_at.elapsed() < UPDATE_INTERVAL {
            return Ok(());
        }
        let view = job_view(&self.connection, &self.path).await?;
        view.set_percent(job.percent.unwrap_or(0)).await?;
        view.set_speed(job.speed).await?;
        self.last = job.clone();
        self.updated_at = std::time::Instant::now();
        Ok(())
    }

    async fn finish(self, app: &tauri::AppHandle, silent: bool) {
        self.listener.abort();
        let canceled = silent
            || self.canceled.load(std::sync::atomic::Ordering::SeqCst)
            || task_is_deleted(app, &self.gid);
        match job_view(&self.connection, &self.path).await {
            Ok(view) => {
                let result = async {
                    // A canceled job must disappear without a finished
                    // notification; Plasma only does that for this error.
                    if canceled {
                        view.set_error(KIO_ERR_USER_CANCELED).await?;
                    }
                    view.terminate("").await
                }
                .await;
                if let Err(error) = result {
                    log::debug!("job_progress:kde-terminate-failed error={error}");
                }
            }
            Err(error) => log::debug!("job_progress:kde-terminate-failed error={error}"),
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for KdeJob {
    fn drop(&mut self) {
        // The listener holds its own connection clone; without this the view
        // would outlive the pump and linger in the task manager.
        self.listener.abort();
    }
}

#[cfg(target_os = "linux")]
fn task_is_deleted(app: &tauri::AppHandle, gid: &str) -> bool {
    use tauri::Manager;
    app.try_state::<crate::services::tasks::TaskServiceState>()
        .is_some_and(|state| state.0.tasks.is_deleted(gid))
}

#[cfg(target_os = "linux")]
async fn job_view<'a>(
    connection: &'a zbus::Connection,
    path: &zbus::zvariant::OwnedObjectPath,
) -> zbus::Result<JobViewProxy<'a>> {
    JobViewProxy::builder(connection)
        .destination(KDE_JOB_VIEW_SERVICE)?
        .path(path.clone())?
        .build()
        .await
}

#[cfg(target_os = "linux")]
async fn set_suspended(
    connection: &zbus::Connection,
    path: &zbus::zvariant::OwnedObjectPath,
    suspended: bool,
) -> zbus::Result<()> {
    job_view(connection, path)
        .await?
        .set_suspended(suspended)
        .await
}

#[cfg(target_os = "linux")]
async fn spawn_control_listener(
    app: tauri::AppHandle,
    connection: zbus::Connection,
    path: zbus::zvariant::OwnedObjectPath,
    gid: String,
    canceled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<tokio::task::JoinHandle<()>, String> {
    use futures_util::StreamExt;

    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(KDE_JOB_VIEW_INTERFACE)
        .map_err(|error| error.to_string())?
        .path(path.clone())
        .map_err(|error| error.to_string())?
        .build();
    let signals = zbus::MessageStream::for_match_rule(rule, &connection, Some(8))
        .await
        .map_err(|error| error.to_string())?;
    let view_connection = connection.clone();
    Ok(tokio::spawn(async move {
        futures_util::pin_mut!(signals);
        while let Some(message) = signals.next().await {
            let Ok(message) = message else {
                break;
            };
            let Some(member) = message
                .header()
                .member()
                .map(|name| name.as_str().to_owned())
            else {
                continue;
            };
            match member.as_str() {
                "cancelRequested" => {
                    log::info!("job_progress:stop-requested gid={gid}");
                    canceled.store(true, std::sync::atomic::Ordering::SeqCst);
                    stop_task(&app, &gid).await;
                }
                "suspendRequested" => {
                    log::info!("job_progress:pause-requested gid={gid}");
                    if pause_task(&app, &gid).await {
                        if let Err(error) = set_suspended(&view_connection, &path, true).await {
                            log::debug!("job_progress:kde-suspend-failed error={error}");
                        }
                    }
                }
                "resumeRequested" => {
                    log::info!("job_progress:resume-requested gid={gid}");
                    resume_task(&app, &gid).await;
                    if let Err(error) = set_suspended(&view_connection, &path, false).await {
                        log::debug!("job_progress:kde-resume-failed error={error}");
                    }
                }
                _ => {}
            }
        }
    }))
}

// ── Windows progress toast ──────────────────────────────────────────
//
// One toast per download, identified by a per-task tag. `NotificationData`
// replaces its values; the sequence number must increase with every update
// or Windows ignores it. A toast whose size is unknown uses the
// `indeterminate` progress state, so it is rebuilt when that state changes.

#[cfg(windows)]
struct WindowsToast {
    app: tauri::AppHandle,
    app_id: String,
    stop_label: String,
    tag: windows::core::HSTRING,
    notifier: windows::UI::Notifications::ToastNotifier,
    toast: windows::UI::Notifications::ToastNotification,
    sequence: u32,
    determinate: bool,
    last: JobProgress,
    updated_at: std::time::Instant,
}

#[cfg(windows)]
impl WindowsToast {
    fn begin(
        app: &tauri::AppHandle,
        config: &RuntimeConfig,
        job: &JobProgress,
    ) -> Result<Self, String> {
        use windows::UI::Notifications::ToastNotificationManager;

        let stop_label = rust_i18n::t!(
            "notification.stop",
            locale = &crate::i18n::resolve_preferred_locale(&config.locale)
        )
        .to_string();
        let app_id = if super::notification::development_build() {
            tauri_winrt_notification::Toast::POWERSHELL_APP_ID.to_owned()
        } else {
            app.config().identifier.clone()
        };
        let tag = windows::core::HSTRING::from(format!("rayburst-{}", job.gid));
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(
            &windows::core::HSTRING::from(app_id.as_str()),
        )
        .map_err(|error| error.to_string())?;
        let mut sequence = 1;
        let toast = create_toast(app, &app_id, &stop_label, &tag, job, &mut sequence)?;
        Ok(Self {
            app: app.clone(),
            app_id,
            stop_label,
            tag,
            notifier,
            toast,
            sequence,
            determinate: job.percent.is_some(),
            last: job.clone(),
            updated_at: std::time::Instant::now(),
        })
    }

    fn update(&mut self, job: &JobProgress) {
        let determinate = job.percent.is_some();
        if determinate != self.determinate {
            if let Err(error) = self.rebuild(job) {
                log::debug!("job_progress:toast-rebuild-failed error={error}");
            }
            return;
        }
        let throttle = self.last.title == job.title
            && self.last.percent == job.percent
            && self.updated_at.elapsed() < UPDATE_INTERVAL;
        if throttle {
            return;
        }
        let data = match self.data(job) {
            Ok(data) => data,
            Err(error) => {
                log::debug!("job_progress:toast-update-failed error={error}");
                return;
            }
        };
        if let Err(error) = self.notifier.UpdateWithTag(&data, &self.tag) {
            log::debug!("job_progress:toast-update-failed error={error}");
            return;
        }
        self.last = job.clone();
        self.updated_at = std::time::Instant::now();
    }

    fn rebuild(&mut self, job: &JobProgress) -> Result<(), String> {
        let toast = create_toast(
            &self.app,
            &self.app_id,
            &self.stop_label,
            &self.tag,
            job,
            &mut self.sequence,
        )?;
        self.toast = toast;
        self.determinate = job.percent.is_some();
        self.last = job.clone();
        self.updated_at = std::time::Instant::now();
        Ok(())
    }

    fn data(
        &mut self,
        job: &JobProgress,
    ) -> Result<windows::UI::Notifications::NotificationData, String> {
        let data = notification_data(job, self.sequence)?;
        self.sequence += 1;
        Ok(data)
    }
}

#[cfg(windows)]
impl Drop for WindowsToast {
    fn drop(&mut self) {
        if let Err(error) = self.notifier.Hide(&self.toast) {
            log::debug!("job_progress:toast-hide-failed error={error}");
        }
    }
}

#[cfg(windows)]
fn notification_data(
    job: &JobProgress,
    sequence: u32,
) -> Result<windows::UI::Notifications::NotificationData, String> {
    use windows::core::HSTRING;
    use windows::Foundation::Collections::StringMap;
    use windows::UI::Notifications::NotificationData;

    let values = StringMap::new().map_err(|error| error.to_string())?;
    for (key, value) in [
        ("progressTitle", job.title.as_str()),
        ("progressStatus", job.status.as_str()),
    ] {
        values
            .Insert(&HSTRING::from(key), &HSTRING::from(value))
            .map_err(|error| error.to_string())?;
    }
    if let Some(percent) = job.percent {
        let percent = percent.min(100);
        values
            .Insert(
                &HSTRING::from("progressValue"),
                &HSTRING::from(format!("{:.3}", f64::from(percent) / 100.0)),
            )
            .map_err(|error| error.to_string())?;
        values
            .Insert(
                &HSTRING::from("progressValueString"),
                &HSTRING::from(format!("{percent}%")),
            )
            .map_err(|error| error.to_string())?;
    }
    NotificationData::CreateNotificationDataWithValuesAndSequenceNumber(&values, sequence)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn create_toast(
    app: &tauri::AppHandle,
    app_id: &str,
    stop_label: &str,
    tag: &windows::core::HSTRING,
    job: &JobProgress,
    sequence: &mut u32,
) -> Result<windows::UI::Notifications::ToastNotification, String> {
    use windows::core::{IInspectable, Interface, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{
        ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
    };

    let progress = if job.percent.is_some() {
        "<progress status=\"determinate\" value=\"{progressValue}\" \
         valueString=\"{progressValueString}\"/>"
    } else {
        "<progress status=\"indeterminate\"/>"
    };
    let xml = format!(
        "<toast><visual><binding template=\"ToastGeneric\">\
         <text id=\"1\">{{progressTitle}}</text>\
         <text id=\"2\">{{progressStatus}}</text>\
         {progress}</binding></visual>\
         <actions><action content=\"{}\" arguments=\"stop:{}\"/></actions>\
         <audio silent=\"true\"/></toast>",
        escape_xml(stop_label),
        job.gid
    );
    let document = XmlDocument::new().map_err(|error| error.to_string())?;
    document
        .LoadXml(&HSTRING::from(xml))
        .map_err(|error| error.to_string())?;
    let toast =
        ToastNotification::CreateToastNotification(&document).map_err(|error| error.to_string())?;
    toast.SetTag(tag).map_err(|error| error.to_string())?;

    let handle = app.clone();
    toast
        .Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, args| {
                if let Some(args) = args.as_ref() {
                    if let Ok(args) = args.cast::<ToastActivatedEventArgs>() {
                        if let Ok(arguments) = args.Arguments() {
                            let arguments = arguments.to_string_lossy();
                            if let Some(target) = arguments.strip_prefix("stop:") {
                                let target = target.to_owned();
                                log::info!("job_progress:stop-requested gid={target}");
                                let app = handle.clone();
                                tauri::async_runtime::spawn(async move {
                                    stop_task(&app, &target).await;
                                });
                            }
                        }
                    }
                }
                Ok(())
            },
        ))
        .map_err(|error| error.to_string())?;

    let data = notification_data(job, *sequence)?;
    *sequence += 1;
    toast.SetData(&data).map_err(|error| error.to_string())?;

    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
        .map_err(|error| error.to_string())?;
    notifier.Show(&toast).map_err(|error| error.to_string())?;
    Ok(toast)
}

#[cfg(windows)]
fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
