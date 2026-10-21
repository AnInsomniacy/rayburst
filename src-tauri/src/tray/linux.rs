//! Native StatusNotifierItem activation, menus and panel reconnection.
use futures_util::StreamExt;
use ksni::TrayMethods;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::AppHandle;
use zbus::proxy;

use super::FloatMode;

const ACTIONS: [(&str, &str); 5] = [
    ("show", "Show Rayburst"),
    ("tray-new-task", "New Task"),
    ("tray-resume-all", "Resume All"),
    ("tray-pause-all", "Pause All"),
    ("tray-quit", "Quit"),
];

#[derive(Default)]
struct Model {
    title: String,
    tooltip: String,
    active: bool,
    hidden: bool,
    labels: HashMap<String, String>,
    float_mode: FloatMode,
}

/// A panel restarting, after resume or unlock, brings the watcher back within
/// moments; only a tray that stays gone leaves the window as the way back in.
///
/// A screen lock drops the StatusNotifierWatcher until unlock. Wait that out,
/// then give the panel this long to come back before showing the window. When
/// the lock cannot be read, show after the grace so a dead tray still has a way in.
const WATCHER_GRACE: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionLock {
    Locked,
    Unlocked,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoreStep {
    Grace,
    WaitForUnlock,
    Show,
    Stay,
}

fn on_watcher_offline(lock: SessionLock) -> RestoreStep {
    match lock {
        SessionLock::Locked => RestoreStep::WaitForUnlock,
        SessionLock::Unlocked | SessionLock::Unknown => RestoreStep::Grace,
    }
}

fn after_unlock(generation_current: bool, lock: SessionLock) -> RestoreStep {
    if generation_current {
        on_watcher_offline(lock)
    } else {
        RestoreStep::Stay
    }
}

fn after_grace(generation_current: bool, lock: SessionLock) -> RestoreStep {
    if !generation_current {
        return RestoreStep::Stay;
    }
    match lock {
        SessionLock::Locked => RestoreStep::WaitForUnlock,
        SessionLock::Unlocked | SessionLock::Unknown => RestoreStep::Show,
    }
}

struct NativeTray {
    app: AppHandle,
    model: Arc<Mutex<Model>>,
    /// Counts watcher changes, so a return cancels a pending window.
    watcher: Arc<AtomicU64>,
}

impl ksni::Tray for NativeTray {
    fn id(&self) -> String {
        crate::APP_ID.into()
    }
    fn title(&self) -> String {
        let model = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if model.title.is_empty() {
            "Rayburst".into()
        } else {
            model.title.clone()
        }
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        let description = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .tooltip
            .clone();
        ksni::ToolTip {
            title: self.title(),
            description,
            ..Default::default()
        }
    }
    fn status(&self) -> ksni::Status {
        let hidden = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .hidden;
        // Hosts leave passive items out of the panel.
        if hidden {
            ksni::Status::Passive
        } else {
            ksni::Status::Active
        }
    }
    fn activate(&mut self, _: i32, _: i32) {
        super::request_main_window(&self.app, "tray-activate", true);
    }
    fn watcher_online(&self) {
        self.watcher.fetch_add(1, Ordering::SeqCst);
        log::info!("tray:watcher-online");
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        log::warn!("tray:watcher-offline reason={reason:?}");
        let change = self.watcher.fetch_add(1, Ordering::SeqCst) + 1;
        let (app, watcher) = (self.app.clone(), self.watcher.clone());
        tauri::async_runtime::spawn(async move {
            postpone_window_while_locked(app, watcher, change).await;
        });
        true
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let active = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active;
        let image = super::tray_icon_image_with_activity(active);
        let data = image
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| [a, r, g, b])
            .collect();
        vec![ksni::Icon {
            width: image.width() as i32,
            height: image.height() as i32,
            data,
        }]
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let model = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let label = |id: &str, fallback: &str| {
            model
                .labels
                .get(id)
                .cloned()
                .unwrap_or_else(|| fallback.into())
        };
        let mut menu: Vec<ksni::MenuItem<Self>> = ACTIONS
            .iter()
            .map(|&(id, fallback)| {
                ksni::menu::StandardItem {
                    label: label(id, fallback),
                    activate: Box::new(move |tray: &mut Self| {
                        super::dispatch_menu_action(&tray.app, id)
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect();
        let float_menu = ksni::menu::SubMenu {
            label: label("tray-float", super::FLOAT_MENU_LABEL),
            submenu: vec![ksni::menu::RadioGroup {
                selected: FloatMode::ALL
                    .iter()
                    .position(|mode| *mode == model.float_mode)
                    .unwrap_or_default(),
                select: Box::new(|tray: &mut Self, index| {
                    let Some(&mode) = FloatMode::ALL.get(index) else {
                        return;
                    };
                    tray.model
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .float_mode = mode;
                    if let Err(error) = crate::services::float_window::set_mode(&tray.app, mode) {
                        log::warn!("tray:float-mode-failed error={error}");
                    }
                }),
                options: FloatMode::ALL
                    .iter()
                    .map(|&mode| ksni::menu::RadioItem {
                        label: label(mode.menu_id(), super::float_mode_label(mode)),
                        ..Default::default()
                    })
                    .collect(),
            }
            .into()],
            ..Default::default()
        };
        // Before Quit, as in the other platforms' menus.
        menu.insert(menu.len() - 1, float_menu.into());
        menu
    }
}

pub struct TrayMenuState {
    model: Arc<Mutex<Model>>,
    handle: Arc<Mutex<Option<ksni::Handle<NativeTray>>>>,
}

impl TrayMenuState {
    fn refresh(&self) {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(handle) = handle {
            tauri::async_runtime::spawn(async move {
                let _ = handle.update(|_| {}).await;
            });
        }
    }
    pub fn set_title(&self, title: &str) {
        self.model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .title = title.into();
        self.refresh();
    }
    pub fn set_visible(&self, visible: bool) {
        self.model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .hidden = !visible;
        self.refresh();
    }
    pub fn set_activity(&self, active: bool, tooltip: &str) {
        {
            let mut model = self
                .model
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            model.active = active;
            model.tooltip = tooltip.into();
        }
        self.refresh();
    }
    pub fn set_float_mode(&self, mode: FloatMode) {
        self.model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .float_mode = mode;
        self.refresh();
    }
    pub fn set_labels(&self, labels: &serde_json::Value) {
        if let Some(labels) = labels.as_object() {
            let mut model = self
                .model
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for (id, text) in labels {
                if let Some(text) = text.as_str() {
                    model.labels.insert(id.clone(), text.into());
                }
            }
        }
        self.refresh();
    }
}

pub fn setup_tray(app: &AppHandle) -> Result<TrayMenuState, Box<dyn std::error::Error>> {
    let state = TrayMenuState {
        model: Arc::new(Mutex::new(Model {
            hidden: !crate::read_pref_bool(app, "showTrayIcon", true),
            float_mode: crate::services::float_window::saved_mode(app),
            ..Model::default()
        })),
        handle: Arc::default(),
    };
    let tray = NativeTray {
        app: app.clone(),
        model: state.model.clone(),
        watcher: Arc::default(),
    };
    let destination = state.handle.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match tray.assume_sni_available(true).spawn().await {
            Ok(handle) => {
                *destination
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle);
            }
            Err(error) => {
                log::error!("tray:registration-failed error={error}");
                super::request_main_window(&app, "tray-unavailable", true);
            }
        }
    });
    Ok(state)
}

async fn postpone_window_while_locked(app: AppHandle, watcher: Arc<AtomicU64>, change: u64) {
    let mut session = open_lock_session().await;
    let mut step = on_watcher_offline(session_lock(&mut session).await);
    loop {
        step = match step {
            RestoreStep::Stay => return,
            RestoreStep::Show => {
                super::request_main_window(&app, "tray-watcher-offline", true);
                return;
            }
            RestoreStep::WaitForUnlock => {
                log::info!("tray:watcher-offline waiting-for-unlock");
                let watch = match &session {
                    Some((connection, path)) => {
                        watch_until_unlocked(connection, path, &watcher, change).await
                    }
                    None => UnlockWatch::Failed,
                };
                if watch == UnlockWatch::Failed {
                    session = open_lock_session().await;
                }
                // Read the lock, then the generation. A watcher that returns
                // during the lock query must still cancel this show.
                let lock = session_lock(&mut session).await;
                let step = after_unlock(watcher.load(Ordering::SeqCst) == change, lock);
                if step == RestoreStep::Grace {
                    log::info!("tray:watcher-offline grace-after-unlock");
                }
                if watch == UnlockWatch::Failed && step == RestoreStep::WaitForUnlock {
                    tokio::time::sleep(WATCHER_GRACE).await;
                }
                step
            }
            RestoreStep::Grace => {
                tokio::time::sleep(WATCHER_GRACE).await;
                let lock = session_lock(&mut session).await;
                after_grace(watcher.load(Ordering::SeqCst) == change, lock)
            }
        };
    }
}

type LockSession = (zbus::Connection, zbus::zvariant::OwnedObjectPath);

async fn open_lock_session() -> Option<LockSession> {
    let connection = match zbus::Connection::system().await {
        Ok(connection) => connection,
        Err(error) => {
            log::warn!("tray:session-lock-unknown error={error}");
            return None;
        }
    };
    match session_path(&connection).await {
        Ok(path) => Some((connection, path)),
        Err(error) => {
            log::warn!("tray:session-lock-unknown error={error}");
            None
        }
    }
}

async fn session_lock(session: &mut Option<LockSession>) -> SessionLock {
    for attempt in 0..2 {
        match read_locked_hint(session).await {
            Ok(Some(true)) => return SessionLock::Locked,
            Ok(Some(false)) => return SessionLock::Unlocked,
            Ok(None) => {}
            Err(error) => log::warn!("tray:session-lock-unknown error={error}"),
        }
        if attempt == 0 {
            *session = open_lock_session().await;
        }
    }
    SessionLock::Unknown
}

async fn read_locked_hint(session: &Option<LockSession>) -> zbus::Result<Option<bool>> {
    let Some((connection, path)) = session else {
        return Ok(None);
    };
    locked_hint(connection, path).await.map(Some)
}

async fn locked_hint(
    connection: &zbus::Connection,
    path: &zbus::zvariant::OwnedObjectPath,
) -> zbus::Result<bool> {
    session_proxy(connection, path).await?.locked_hint().await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UnlockWatch {
    Unlocked,
    Superseded,
    Failed,
}

async fn watch_until_unlocked(
    connection: &zbus::Connection,
    path: &zbus::zvariant::OwnedObjectPath,
    watcher: &AtomicU64,
    generation: u64,
) -> UnlockWatch {
    if watcher.load(Ordering::SeqCst) != generation {
        return UnlockWatch::Superseded;
    }
    let session = match session_proxy(connection, path).await {
        Ok(session) => session,
        Err(error) => {
            log::warn!("tray:session-unlock-watch-failed error={error}");
            return UnlockWatch::Failed;
        }
    };
    // Subscribe first so an unlock between the two calls is not missed.
    let mut changes = session.receive_locked_hint_changed().await;
    match session.locked_hint().await {
        Ok(false) => return UnlockWatch::Unlocked,
        Ok(true) => {}
        Err(error) => {
            log::warn!("tray:session-unlock-watch-failed error={error}");
            return UnlockWatch::Failed;
        }
    }
    loop {
        tokio::select! {
            next = changes.next() => {
                match next {
                    Some(property) => match property.get().await {
                        Ok(false) => return UnlockWatch::Unlocked,
                        Ok(true) => {}
                        Err(error) => {
                            log::warn!("tray:session-unlock-watch-failed error={error}");
                            return UnlockWatch::Failed;
                        }
                    },
                    None => {
                        log::warn!("tray:session-unlock-watch-failed error=ended");
                        return UnlockWatch::Failed;
                    }
                }
            }
            _ = tokio::time::sleep(WATCHER_GRACE) => {
                if watcher.load(Ordering::SeqCst) != generation {
                    return UnlockWatch::Superseded;
                }
                // A new proxy, so this read does not borrow the signal stream.
                match locked_hint(connection, path).await {
                    Ok(false) => return UnlockWatch::Unlocked,
                    Ok(true) => {}
                    Err(error) => {
                        log::warn!("tray:session-unlock-watch-failed error={error}");
                        return UnlockWatch::Failed;
                    }
                }
            }
        }
    }
}

async fn session_path(
    connection: &zbus::Connection,
) -> zbus::Result<zbus::zvariant::OwnedObjectPath> {
    let manager = LoginManagerProxy::new(connection).await?;
    match manager.get_session_by_pid(std::process::id()).await {
        Ok(path) => Ok(path),
        Err(error) if no_session_for_pid(&error) => {
            log::info!("tray:session-by-pid-unavailable error={error}");
            graphical_session(connection, &manager).await
        }
        Err(error) => Err(error),
    }
}

async fn session_proxy<'a>(
    connection: &'a zbus::Connection,
    path: &zbus::zvariant::OwnedObjectPath,
) -> zbus::Result<LoginSessionProxy<'a>> {
    LoginSessionProxy::builder(connection)
        .path(path.clone())?
        .build()
        .await
}

fn no_session_for_pid(error: &zbus::Error) -> bool {
    error.to_string().contains("NoSessionForPID")
}

async fn graphical_session(
    connection: &zbus::Connection,
    manager: &LoginManagerProxy<'_>,
) -> zbus::Result<zbus::zvariant::OwnedObjectPath> {
    let user_path = manager.get_user(user_id()).await?;
    let user = LoginUserProxy::builder(connection)
        .path(user_path)?
        .build()
        .await?;
    let (session_id, path) = user.display().await?;
    if session_id.is_empty() || path.as_str() == "/" {
        return Err(zbus::Error::Failure("no graphical logind session".into()));
    }
    log::info!("tray:session-lock display-session id={session_id}");
    Ok(path)
}

fn user_id() -> u32 {
    // getuid only reports the real user id.
    unsafe { libc::getuid() }
}

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1",
    gen_blocking = false
)]
trait LoginManager {
    #[zbus(name = "GetSessionByPID")]
    fn get_session_by_pid(&self, pid: u32) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    fn get_user(&self, uid: u32) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.login1.User",
    default_service = "org.freedesktop.login1",
    gen_blocking = false
)]
trait LoginUser {
    #[zbus(property)]
    fn display(&self) -> zbus::Result<(String, zbus::zvariant::OwnedObjectPath)>;
}

#[proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    gen_blocking = false
)]
trait LoginSession {
    #[zbus(property)]
    fn locked_hint(&self) -> zbus::Result<bool>;
}

#[cfg(test)]
mod tests {
    use super::{after_grace, after_unlock, on_watcher_offline, RestoreStep, SessionLock};

    #[test]
    fn unlocked_and_current_shows_the_window() {
        assert_eq!(after_grace(true, SessionLock::Unlocked), RestoreStep::Show);
    }

    #[test]
    fn locked_session_waits_for_unlock() {
        assert_eq!(
            on_watcher_offline(SessionLock::Locked),
            RestoreStep::WaitForUnlock
        );
    }

    #[test]
    fn unlock_with_a_new_watcher_generation_stays_hidden() {
        assert_eq!(
            after_unlock(false, SessionLock::Unlocked),
            RestoreStep::Stay
        );
    }

    #[test]
    fn relock_after_unlock_keeps_waiting() {
        assert_eq!(
            after_unlock(true, SessionLock::Locked),
            RestoreStep::WaitForUnlock
        );
    }

    #[test]
    fn unknown_lock_shows_like_an_unlocked_session() {
        assert_eq!(after_grace(true, SessionLock::Unknown), RestoreStep::Show);
        assert_eq!(
            on_watcher_offline(SessionLock::Unknown),
            on_watcher_offline(SessionLock::Unlocked)
        );
    }

    #[test]
    fn grace_with_a_new_watcher_generation_stays_hidden() {
        assert_eq!(after_grace(false, SessionLock::Unlocked), RestoreStep::Stay);
        assert_eq!(after_grace(false, SessionLock::Locked), RestoreStep::Stay);
    }
}
