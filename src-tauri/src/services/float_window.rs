//! The floating window (#669, #718, #402, #726).
//!
//! A small orb that stays on top with the overall speed and progress, so
//! downloads can be followed without the main window. A click opens a panel
//! beside it to follow and control each download, and links or torrents dropped
//! on it open the add window. Its mode keeps it always, only while files
//! download, or never; the pump opens and closes both windows accordingly and
//! feeds them each snapshot.
//!
//! The orb never changes its size, and the panel is a window of its own, built
//! hidden with the orb; neither has to move or resize the other to open, so
//! nothing jumps.
//!
//! On Windows the orb steps aside while an application in fullscreen, a video
//! or a game, fills its screen, and returns when that ends. macOS keeps windows
//! on all desktops out of fullscreen spaces by itself.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_store::StoreExt;

use super::stat::StatUpdate;
use crate::aria2::types::Aria2Task;
use crate::error::AppError;

pub const LABEL: &str = "float";
pub const PANEL_LABEL: &str = "float-panel";
const UPDATE_EVENT: &str = "float:update";
/// Asks the orb to animate out before it is destroyed.
const LEAVE_EVENT: &str = "float:leave";
const PANEL_OPEN_EVENT: &str = "float-panel:open";
const PANEL_CLOSE_EVENT: &str = "float-panel:close";
/// Tells every window that the mode changed outside the settings.
const MODE_EVENT: &str = "float-window:mode";
/// Size of the orb window in logical pixels: the 52 px orb and the margin
/// holding its shadow, `FloatWindow.vue`.
const ORB_SIZE: f64 = 68.0;
const ORB_INSET: f64 = 8.0;
/// Margin holding the panel's shadow, `FloatPanel.vue`.
const PANEL_INSET: f64 = 12.0;
/// Space between the orb and the panel as drawn.
const PANEL_GAP: f64 = 8.0;
/// Distance from the corner of the screen where the orb first appears.
const MARGIN: f64 = 16.0;
/// Length of the orb's leave animation.
const LEAVE: Duration = Duration::from_millis(200);
/// A click on the orb right after the panel closed on losing focus to that
/// very click must not open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(300);
/// Downloads the panel lists; the main window has the rest.
const MAX_TASKS: usize = 20;
/// How long the orb stays after the last download in `downloading` mode: long
/// enough to show its check.
const LINGER: Duration = Duration::from_millis(2400);

/// When the orb shows, the `floatWindow` preference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Always,
    /// While files download; paused and seeding tasks do not count.
    Downloading,
    #[default]
    #[serde(other)]
    Off,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Always, Mode::Downloading, Mode::Off];

    /// Menu item ID, shared by the tray menus.
    pub fn menu_id(self) -> &'static str {
        match self {
            Mode::Always => "float-always",
            Mode::Downloading => "float-downloading",
            Mode::Off => "float-off",
        }
    }

    pub fn from_menu_id(id: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|mode| mode.menu_id() == id)
    }
}

/// The window was put away while the main window rested in the tray, so the
/// implicit exit that follows must not end the process.
static RESIDENT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Default)]
struct Panel {
    open: bool,
    closed_at: Option<Instant>,
    /// Where the orb sits relative to the open panel, in physical pixels, so
    /// dragging the panel takes the orb along.
    orb_offset: Option<(i32, i32)>,
}

static PANEL: Mutex<Panel> = Mutex::new(Panel {
    open: false,
    closed_at: None,
    orb_offset: None,
});

fn panel() -> std::sync::MutexGuard<'static, Panel> {
    PANEL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Update<'a> {
    stat: &'a StatUpdate,
    /// Overall progress of the active downloads in percent, when measurable.
    progress: Option<u64>,
    /// Active downloads first, then waiting and paused ones.
    tasks: Vec<Aria2Task>,
}

/// Lives in the pump; decides each tick whether the windows exist.
#[derive(Default)]
pub struct FloatWindow {
    opening: Arc<AtomicBool>,
    leaving: Arc<AtomicBool>,
    /// The mode the tray menus show.
    shown_mode: Mutex<Option<Mode>>,
    /// When the last download stopped, in `downloading` mode.
    quiet_since: Mutex<Option<Instant>>,
    /// The orb is hidden behind an application in fullscreen.
    covered: Arc<AtomicBool>,
}

impl FloatWindow {
    /// `tasks` holds the queued tasks of the same snapshot as `stat`.
    pub fn sync(&self, app: &AppHandle, mode: Mode, stat: &StatUpdate, tasks: &[Aria2Task]) {
        let mut shown = self
            .shown_mode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *shown != Some(mode) {
            crate::tray::set_float_mode(app, mode);
            *shown = Some(mode);
        }
        drop(shown);
        let wanted = self.wanted(app, mode, tasks);
        match (wanted, app.get_webview_window(LABEL)) {
            (true, Some(window)) if !self.leaving.load(Ordering::Acquire) => {
                if self.cover(app, &window) {
                    return;
                }
                let active: Vec<_> = tasks
                    .iter()
                    .filter(|task| task.status == "active")
                    .cloned()
                    .collect();
                let update = Update {
                    stat,
                    progress: super::stat::task_progress(&active),
                    tasks: queue(tasks),
                };
                for label in [LABEL, PANEL_LABEL] {
                    if let Err(error) = app.emit_to(label, UPDATE_EVENT, &update) {
                        log::debug!("float_window:emit-failed label={label} error={error}");
                    }
                }
            }
            (true, None) if !fullscreen_in_front(app, None) => self.open(app),
            (false, Some(window)) => self.leave(app, window),
            _ => {}
        }
    }

    /// Lightweight mode releases every webview into the tray: without the
    /// main window there is no orb either, until the main window returns,
    /// unless the user keeps the orb there.
    fn wanted(&self, app: &AppHandle, mode: Mode, tasks: &[Aria2Task]) -> bool {
        let busy = tasks.iter().any(downloading) || panel().open;
        let mut quiet = self
            .quiet_since
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mode = if app.get_webview_window("main").is_some()
            || crate::read_pref_bool(app, "floatWindowInLightweight", false)
        {
            mode
        } else {
            Mode::Off
        };
        wants_orb(mode, busy, exists(app), &mut quiet, Instant::now())
    }

    /// Hides the orb while an application in fullscreen is in front of it and
    /// brings it back without taking the focus; true while it is hidden.
    fn cover(&self, app: &AppHandle, window: &WebviewWindow) -> bool {
        let covered = fullscreen_in_front(app, Some(window));
        if self.covered.swap(covered, Ordering::AcqRel) != covered {
            if covered {
                close_panel(app);
            }
            show_inactive(window, !covered);
            log::info!("float_window:covered covered={covered}");
        }
        covered
    }

    fn open(&self, app: &AppHandle) {
        if self.opening.swap(true, Ordering::AcqRel) {
            return;
        }
        let app = app.clone();
        let opening = Arc::clone(&self.opening);
        // Building a window waits on the event loop; never do it on that thread.
        tauri::async_runtime::spawn_blocking(move || {
            for (label, config) in [(LABEL, orb_config(&app)), (PANEL_LABEL, panel_config(&app))] {
                let built = config
                    .and_then(|config| WebviewWindowBuilder::from_config(&app, &config))
                    .and_then(WebviewWindowBuilder::build);
                match built {
                    Ok(window) => {
                        super::add_window::without_system_transitions(&window);
                        if label == LABEL {
                            restore_orb_position(&app, window);
                        }
                        log::info!("float_window:opened label={label}");
                    }
                    Err(error) => {
                        log::warn!("float_window:open-failed label={label} error={error}")
                    }
                }
            }
            opening.store(false, Ordering::Release);
        });
    }

    /// Lets the orb animate out, then destroys both windows.
    fn leave(&self, app: &AppHandle, window: WebviewWindow) {
        if self.leaving.swap(true, Ordering::AcqRel) {
            return;
        }
        close_panel(app);
        if let Err(error) = window.emit_to(LABEL, LEAVE_EVENT, ()) {
            log::debug!("float_window:emit-failed label={LABEL} error={error}");
        }
        let app = app.clone();
        let leaving = Arc::clone(&self.leaving);
        let covered = Arc::clone(&self.covered);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(LEAVE).await;
            RESIDENT.store(app.get_webview_window("main").is_none(), Ordering::Relaxed);
            // The panel first, so the orb is the last window when it goes.
            for label in [PANEL_LABEL, LABEL] {
                if let Some(window) = app.get_webview_window(label) {
                    if let Err(error) = window.destroy() {
                        log::warn!("float_window:destroy-failed label={label} error={error}");
                    }
                }
            }
            *panel() = Panel::default();
            covered.store(false, Ordering::Release);
            leaving.store(false, Ordering::Release);
        });
    }
}

fn downloading(task: &Aria2Task) -> bool {
    task.status == "active" && task.seeder.as_deref() != Some("true")
}

/// In `downloading` mode the orb comes with the first download and leaves a
/// moment after the last one; `busy` also holds it while its panel is open.
/// `quiet` remembers when the orb ran out of downloads.
fn wants_orb(
    mode: Mode,
    busy: bool,
    shown: bool,
    quiet: &mut Option<Instant>,
    now: Instant,
) -> bool {
    if mode != Mode::Downloading || busy || !shown {
        *quiet = None;
        return match mode {
            Mode::Always => true,
            Mode::Downloading => busy,
            Mode::Off => false,
        };
    }
    now.duration_since(*quiet.get_or_insert(now)) < LINGER
}

/// Shows or hides a window without activating it. Asynchronous, so the pump
/// never waits on the thread that owns the window.
fn show_inactive(window: &WebviewWindow, visible: bool) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            ShowWindowAsync, SW_HIDE, SW_SHOWNOACTIVATE,
        };
        if let Ok(hwnd) = window.hwnd() {
            let command = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
            // SAFETY: the handle belongs to a live window; the call only posts.
            unsafe { ShowWindowAsync(hwnd.0 as _, command) };
        }
    }
    #[cfg(not(windows))]
    let _ = if visible {
        window.show()
    } else {
        window.hide()
    };
}

/// Whether the foreground window fills `screen`, as a fullscreen one does; a
/// maximized window leaves the taskbar free.
#[cfg_attr(not(windows), allow(dead_code))]
fn fills(window: Rect, screen: Rect) -> bool {
    window.x <= screen.x
        && window.y <= screen.y
        && window.x + window.width >= screen.x + screen.width
        && window.y + window.height >= screen.y + screen.height
}

/// Whether an application in fullscreen is in front of the orb's screen, or
/// of any screen before the orb exists (#726). Windows reports that one is
/// running; its window must also fill the screen, so a video in fullscreen on
/// another screen leaves the orb in place.
#[cfg(windows)]
fn fullscreen_in_front(app: &AppHandle, orb: Option<&WebviewWindow>) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE,
        QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    };
    let mut state = 0;
    // SAFETY: the pointer is valid for the one value written.
    if unsafe { SHQueryUserNotificationState(&mut state) } != 0
        || !matches!(
            state,
            QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE
        )
    {
        return false;
    }
    // SAFETY: takes no arguments; null when no window has the focus.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut process = 0;
    // SAFETY: the handle came from the system; the pointer is valid.
    unsafe { GetWindowThreadProcessId(foreground, &mut process) };
    if process == std::process::id() {
        return false;
    }
    let mut class = [0u16; 16];
    // SAFETY: the buffer holds the length passed.
    let length = unsafe { GetClassNameW(foreground, class.as_mut_ptr(), class.len() as i32) };
    let class = String::from_utf16_lossy(&class[..usize::try_from(length).unwrap_or(0)]);
    // The desktop fills the screen as well.
    if matches!(class.as_str(), "Progman" | "WorkerW") {
        return false;
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: the handle came from the system; the pointer is valid.
    if unsafe { GetWindowRect(foreground, &mut rect) } == 0 {
        return false;
    }
    let window = Rect {
        x: f64::from(rect.left),
        y: f64::from(rect.top),
        width: f64::from(rect.right - rect.left),
        height: f64::from(rect.bottom - rect.top),
    };
    let screens = match orb {
        Some(orb) => orb.current_monitor().ok().flatten().into_iter().collect(),
        None => app.available_monitors().unwrap_or_default(),
    };
    screens.iter().any(|screen| {
        let (position, size) = (screen.position(), screen.size());
        fills(
            window,
            Rect {
                x: f64::from(position.x),
                y: f64::from(position.y),
                width: f64::from(size.width),
                height: f64::from(size.height),
            },
        )
    })
}

#[cfg(not(windows))]
fn fullscreen_in_front(_: &AppHandle, _: Option<&WebviewWindow>) -> bool {
    false
}

/// Saves a mode picked in the orb's menu or the tray and applies it at once;
/// the windows holding preferences reload them.
pub fn set_mode(app: &AppHandle, mode: Mode) -> Result<(), AppError> {
    let store = app
        .store("config.json")
        .map_err(|error| AppError::Store(error.to_string()))?;
    let mut preferences = store
        .get("preferences")
        .unwrap_or_else(|| serde_json::json!({}));
    let fields = preferences
        .as_object_mut()
        .ok_or_else(|| AppError::Store("preferences are not an object".into()))?;
    fields.insert("floatWindow".into(), serde_json::json!(mode));
    store.set("preferences", preferences.clone());
    store
        .save()
        .map_err(|error| AppError::Store(error.to_string()))?;
    log::info!("float_window:mode mode={mode:?}");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(state) = app.try_state::<super::config::RuntimeConfigState>() {
            if let Err(error) = state.refresh_from_json(&preferences).await {
                log::warn!("float_window:mode-refresh-failed error={error}");
            }
        }
        super::pump::wake(&app);
        if let Err(error) = app.emit(MODE_EVENT, mode) {
            log::debug!("float_window:emit-failed event={MODE_EVENT} error={error}");
        }
    });
    Ok(())
}

/// The mode saved in the preferences, for the tray menus built at startup.
pub fn saved_mode(app: &AppHandle) -> Mode {
    app.store("config.json")
        .ok()
        .and_then(|store| store.get("preferences"))
        .and_then(|preferences| preferences.get("floatWindow").cloned())
        .and_then(|mode| serde_json::from_value(mode).ok())
        .unwrap_or_default()
}

/// Active downloads first, each with only what a row shows.
fn queue(tasks: &[Aria2Task]) -> Vec<Aria2Task> {
    let rank = |task: &Aria2Task| match task.status.as_str() {
        "active" => 0,
        "waiting" => 1,
        _ => 2,
    };
    let mut queue: Vec<_> = tasks
        .iter()
        .filter(|task| matches!(task.status.as_str(), "active" | "waiting" | "paused"))
        .collect();
    queue.sort_by_key(|task| rank(task));
    queue
        .into_iter()
        .take(MAX_TASKS)
        .map(|task| {
            let mut task = task.clone();
            task.files.truncate(1);
            task
        })
        .collect()
}

/// Borderless and transparent on top of other windows, out of the taskbar and
/// on every desktop; the pages draw what shows.
fn floating_config(
    app: &AppHandle,
    label: &str,
) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = super::add_window::secondary_window_config(app, label)?;
    config.center = false;
    config.decorations = false;
    // GTK keeps a window the user cannot resize at least 200x200, which would
    // leave a transparent square around the orb catching clicks (tauri#6125).
    // Without borders the user cannot resize it anyway.
    config.resizable = cfg!(target_os = "linux");
    config.transparent = true;
    config.shadow = false;
    config.always_on_top = true;
    config.skip_taskbar = true;
    config.focus = false;
    config.visible_on_all_workspaces = true;
    config.title_bar_style = tauri::TitleBarStyle::Visible;
    config.hidden_title = false;
    config.traffic_light_position = None;
    Ok(config)
}

/// The orb appears without taking the focus from the window in use, at the
/// bottom right corner of the screen under the pointer until the user moves
/// it; the window-state plugin restores where it was left.
fn orb_config(app: &AppHandle) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = floating_config(app, LABEL)?;
    config.width = ORB_SIZE;
    config.height = ORB_SIZE;
    config.visible = true;
    // The page takes drops itself: the native handler on Windows only passes
    // files on and turns links away.
    config.drag_drop_enabled = false;
    if let Some((x, y)) = orb_corner(app) {
        config.x = Some(x);
        config.y = Some(y);
    }
    Ok(config)
}

/// Where the orb first appears, in logical pixels. The windows are built off
/// the main thread, but the work area is read there: on X11 it comes from the
/// X server, and Xlib calls from another thread crash the process (#725).
fn orb_corner(app: &AppHandle) -> Option<(f64, f64)> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let corner = super::add_window::pointer_monitor(&handle).map(|monitor| {
            let area = monitor.work_area();
            let scale = monitor.scale_factor();
            (
                f64::from(area.position.x) / scale + f64::from(area.size.width) / scale
                    - ORB_SIZE
                    - MARGIN,
                f64::from(area.position.y) / scale + f64::from(area.size.height) / scale
                    - ORB_SIZE
                    - MARGIN,
            )
        });
        let _ = sender.send(corner);
    })
    .ok()?;
    receiver.recv_timeout(Duration::from_secs(2)).ok().flatten()
}

/// Puts the orb back where the user left it. The window-state plugin would do
/// this on the thread that built the window, reading every screen's work area
/// there; on X11 that crashes the process (#725), so it is done on the main
/// thread. The orb is still transparent and empty, so it never shows the move.
fn restore_orb_position(app: &AppHandle, window: WebviewWindow) {
    use tauri_plugin_window_state::{StateFlags, WindowExt};
    let restored = app.run_on_main_thread(move || {
        if let Err(error) = window.restore_state(StateFlags::POSITION) {
            log::warn!("float_window:restore-failed error={error}");
        }
    });
    if let Err(error) = restored {
        log::warn!("float_window:restore-failed error={error}");
    }
}

/// The panel is sized by its page and placed beside the orb as it opens.
fn panel_config(app: &AppHandle) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = floating_config(app, PANEL_LABEL)?;
    config.width = 344.0;
    config.height = 240.0;
    Ok(config)
}

/// Which side of the orb the panel opened on, so it grows out of the orb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSide {
    /// The panel sits above the orb.
    pub above: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Places a panel of `width` × `height` beside the orb: towards the larger
/// half of the screen, centered on the orb and kept on screen. All values in
/// physical pixels; `scale` converts the logical insets.
fn place_panel(
    orb: Rect,
    width: f64,
    height: f64,
    area: Rect,
    scale: f64,
) -> (f64, f64, PanelSide) {
    let orb_inset = ORB_INSET * scale;
    let panel_inset = PANEL_INSET * scale;
    let gap = PANEL_GAP * scale;
    let x = orb.x + (orb.width - width) / 2.0;
    let above_y = orb.y + orb_inset - gap + panel_inset - height;
    let below_y = orb.y + orb.height - orb_inset + gap - panel_inset;
    let room_above = above_y + panel_inset >= area.y;
    let room_below = below_y + height - panel_inset <= area.y + area.height;
    let lower_half = orb.y + orb.height / 2.0 > area.y + area.height / 2.0;
    let above = (lower_half && room_above) || !room_below;
    let y = if above { above_y } else { below_y };
    // The drawn panel stays on screen; its transparent margin may leave it.
    let clamp = |value: f64, length: f64, start: f64, span: f64| {
        value
            .min(start + span - length + panel_inset)
            .max(start - panel_inset)
    };
    (
        clamp(x, width, area.x, area.width),
        clamp(y, height, area.y, area.height),
        PanelSide { above },
    )
}

/// Opens the panel, or closes it when it is open or was just closed by the
/// very click that asks.
pub fn toggle_panel(app: &AppHandle) {
    let mut state = panel();
    if state.open {
        drop(state);
        close_panel(app);
        return;
    }
    if state
        .closed_at
        .is_some_and(|closed| closed.elapsed() < REOPEN_GUARD)
    {
        return;
    }
    state.open = true;
    drop(state);
    if let Err(error) = app.emit_to(PANEL_LABEL, PANEL_OPEN_EVENT, ()) {
        log::warn!("float_window:panel-open-failed error={error}");
        panel().open = false;
    }
}

/// Asks the panel to animate out; it hides itself.
pub fn close_panel(app: &AppHandle) {
    let mut state = panel();
    if !state.open {
        return;
    }
    state.open = false;
    state.closed_at = Some(Instant::now());
    state.orb_offset = None;
    drop(state);
    let _ = app.emit_to(PANEL_LABEL, PANEL_CLOSE_EVENT, ());
}

/// Sizes the panel to its content, given in logical pixels, places it beside
/// the orb and shows it. Main thread only: on X11 the work area it reads
/// comes from the X server (#725).
pub fn present_panel(app: &AppHandle, width: f64, height: f64) -> Result<PanelSide, AppError> {
    let missing = |label: &str| AppError::NotFound(format!("{label} window"));
    let failed = |error: tauri::Error| AppError::Io(format!("float panel: {error}"));
    let orb = app
        .get_webview_window(LABEL)
        .ok_or_else(|| missing(LABEL))?;
    let window = app
        .get_webview_window(PANEL_LABEL)
        .ok_or_else(|| missing(PANEL_LABEL))?;
    if !panel().open {
        return Err(AppError::NotFound("open float panel".into()));
    }
    let position = orb.outer_position().map_err(failed)?;
    let size = orb.outer_size().map_err(failed)?;
    let monitor = orb
        .current_monitor()
        .map_err(failed)?
        .ok_or_else(|| missing("float monitor"))?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let orb_rect = Rect {
        x: f64::from(position.x),
        y: f64::from(position.y),
        width: f64::from(size.width),
        height: f64::from(size.height),
    };
    let area_rect = Rect {
        x: f64::from(area.position.x),
        y: f64::from(area.position.y),
        width: f64::from(area.size.width),
        height: f64::from(area.size.height),
    };
    let (panel_width, panel_height) = ((width * scale).round(), (height * scale).round());
    let (x, y, side) = place_panel(orb_rect, panel_width, panel_height, area_rect, scale);
    let at = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    panel().orb_offset = Some((position.x - at.x, position.y - at.y));
    window
        .set_size(PhysicalSize::new(panel_width as u32, panel_height as u32))
        .map_err(failed)?;
    window.set_position(at).map_err(failed)?;
    if !window.is_visible().unwrap_or(false) {
        window.show().map_err(failed)?;
    }
    window.set_focus().map_err(failed)?;
    Ok(side)
}

/// Dragging the open panel by its header takes the orb along.
pub fn panel_moved(app: &AppHandle, position: PhysicalPosition<i32>) {
    let Some((dx, dy)) = panel().orb_offset else {
        return;
    };
    if let Some(orb) = app.get_webview_window(LABEL) {
        let _ = orb.set_position(PhysicalPosition::new(position.x + dx, position.y + dy));
    }
}

pub fn exists(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some()
}

/// Whether the last window to close was this one, put away while the main
/// window rested in the tray.
pub fn keeps_process(app: &AppHandle) -> bool {
    RESIDENT.load(Ordering::Relaxed) && app.get_webview_window("main").is_none()
}

/// Forgets tray residency once the main window exists again.
pub fn main_window_created() {
    RESIDENT.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::{fills, place_panel, queue, wants_orb, Mode, PanelSide, Rect, LINGER};
    use crate::aria2::types::Aria2Task;
    use std::time::{Duration, Instant};

    #[test]
    fn only_a_window_filling_the_screen_hides_the_orb() {
        let screen = Rect {
            x: 1920.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        };
        let fullscreen = screen;
        let maximized = Rect {
            height: 1392.0,
            ..screen
        };
        let other_screen = Rect { x: 0.0, ..screen };
        assert!(fills(fullscreen, screen));
        assert!(!fills(maximized, screen));
        assert!(!fills(other_screen, screen));
    }

    #[test]
    fn while_downloading_the_orb_leaves_a_moment_after_the_last_download() {
        let start = Instant::now();
        let mut quiet = None;
        let downloading = Mode::Downloading;
        assert!(!wants_orb(downloading, false, false, &mut quiet, start));
        assert!(wants_orb(downloading, true, false, &mut quiet, start));
        assert!(wants_orb(downloading, true, true, &mut quiet, start));
        // The last download stopped: the orb stays to show its check.
        assert!(wants_orb(downloading, false, true, &mut quiet, start));
        let later = start + LINGER - Duration::from_millis(1);
        assert!(wants_orb(downloading, false, true, &mut quiet, later));
        let expired = start + LINGER;
        assert!(!wants_orb(downloading, false, true, &mut quiet, expired));
        // Gone; the next download brings it back and restarts the wait.
        assert!(!wants_orb(downloading, false, false, &mut quiet, expired));
        assert_eq!(quiet, None);
        assert!(wants_orb(Mode::Always, false, false, &mut quiet, start));
        assert!(!wants_orb(Mode::Off, true, true, &mut quiet, start));
    }

    #[test]
    fn modes_read_their_preference_values_and_unknown_ones_turn_the_orb_off() {
        let mode = |value: &str| serde_json::from_value::<Mode>(serde_json::json!(value)).unwrap();
        assert_eq!(mode("always"), Mode::Always);
        assert_eq!(mode("downloading"), Mode::Downloading);
        assert_eq!(mode("sometimes"), Mode::Off);
        assert_eq!(serde_json::json!(Mode::Downloading), "downloading");
    }

    fn task(gid: &str, status: &str) -> Aria2Task {
        Aria2Task {
            gid: gid.into(),
            status: status.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_queue_lists_active_downloads_first_and_drops_finished_ones() {
        let tasks = [
            task("a", "paused"),
            task("b", "complete"),
            task("c", "waiting"),
            task("d", "active"),
        ];
        let gids: Vec<_> = queue(&tasks).into_iter().map(|task| task.gid).collect();
        assert_eq!(gids, ["d", "c", "a"]);
    }

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1040.0,
    };

    fn orb(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 68.0,
            height: 68.0,
        }
    }

    #[test]
    fn the_panel_opens_centered_on_the_orb_towards_the_larger_part_of_the_screen() {
        // Lower half: above the orb, centered on it.
        let middle = orb(900.0, 700.0);
        let (x, y, side) = place_panel(middle, 344.0, 300.0, AREA, 1.0);
        assert_eq!(side, PanelSide { above: true });
        assert_eq!(x + 344.0 / 2.0, middle.x + 68.0 / 2.0);
        assert_eq!(y + 300.0 - 12.0 + 8.0, middle.y + 8.0);

        // Top left corner: below the orb, the drawn panel kept on screen.
        let corner = orb(16.0, 16.0);
        let (x, y, side) = place_panel(corner, 344.0, 300.0, AREA, 1.0);
        assert_eq!(side, PanelSide { above: false });
        assert_eq!(x + 12.0, 0.0);
        assert_eq!(y + 12.0, corner.y + 68.0 - 8.0 + 8.0);

        // Bottom right corner: the drawn panel ends at the screen's edge.
        let (x, _, _) = place_panel(orb(1836.0, 956.0), 344.0, 300.0, AREA, 1.0);
        assert_eq!(x + 344.0 - 12.0, AREA.width);
    }

    #[test]
    fn a_panel_without_room_above_opens_below_and_stays_on_screen() {
        let (x, y, side) = place_panel(orb(1800.0, 300.0), 344.0, 400.0, AREA, 1.0);
        assert!(!side.above);
        assert!(x + 344.0 - 12.0 <= AREA.width);
        assert!(y + 12.0 >= 0.0);
    }
}
