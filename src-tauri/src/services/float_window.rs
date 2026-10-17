//! The floating window (#669, #718, #402, #726).
//!
//! A small bar that stays on top with the overall speed and progress, so
//! downloads can be followed without the main window. A click opens a panel
//! beside it to follow and control each download, and links or torrents dropped
//! on it open the add window. Its mode keeps it always, only while files
//! download, or never; the pump opens and closes both windows accordingly and
//! feeds them each snapshot.
//!
//! The bar never changes its size, and the panel is a window of its own,
//! built when it opens and destroyed when it closes; neither has to move or
//! resize the other, so nothing jumps. Both are opaque: the system draws their
//! corners and shadows and animates them in and out.
//!
//! On Windows the bar steps aside while an application in fullscreen, a video
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
/// Tells every window that the mode changed outside the settings.
const MODE_EVENT: &str = "float-window:mode";
/// Size of the bar window in logical pixels, `FloatBar.vue`.
const BAR_WIDTH: f64 = 132.0;
const BAR_HEIGHT: f64 = 36.0;
/// Space between the bar and the panel.
const PANEL_GAP: f64 = 8.0;
/// Distance from the corner of the screen where the bar first appears.
const MARGIN: f64 = 16.0;
/// A click on the bar right after the panel closed on losing focus to that
/// very click must not open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(300);
/// Downloads the panel lists; the main window has the rest.
const MAX_TASKS: usize = 20;
/// How long the bar stays after the last download in `downloading` mode: long
/// enough to show its check.
const LINGER: Duration = Duration::from_millis(2400);

/// When the bar shows, the `floatWindow` preference.
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
    /// Where the bar sits relative to the open panel, in physical pixels, so
    /// dragging the panel takes the bar along.
    bar_offset: Option<(i32, i32)>,
}

static PANEL: Mutex<Panel> = Mutex::new(Panel {
    open: false,
    closed_at: None,
    bar_offset: None,
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
    /// The mode the tray menus show.
    shown_mode: Mutex<Option<Mode>>,
    /// When the last download stopped, in `downloading` mode.
    quiet_since: Mutex<Option<Instant>>,
    /// The bar is hidden behind an application in fullscreen.
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
            (true, Some(window)) => {
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
            (false, Some(_)) => self.close(app),
            _ => {}
        }
    }

    fn wanted(&self, app: &AppHandle, mode: Mode, tasks: &[Aria2Task]) -> bool {
        let busy = tasks.iter().any(downloading) || panel().open;
        let mut quiet = self
            .quiet_since
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        wants_bar(mode, busy, exists(app), &mut quiet, Instant::now())
    }

    /// Hides the bar while an application in fullscreen is in front of it and
    /// brings it back without taking the focus; true while it is hidden.
    fn cover(&self, app: &AppHandle, window: &WebviewWindow) -> bool {
        let covered = fullscreen_in_front(app, Some(window));
        if self.covered.swap(covered, Ordering::AcqRel) != covered {
            if covered {
                close_panel(app);
            }
            super::native_window::show_inactive(window, !covered);
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
            match build(&app, bar_config(&app)) {
                Ok(window) => restore_bar_position(&app, window),
                Err(error) => log::warn!("float_window:open-failed label={LABEL} error={error}"),
            }
            opening.store(false, Ordering::Release);
        });
    }

    /// Destroys both windows as they animate out.
    fn close(&self, app: &AppHandle) {
        close_panel(app);
        RESIDENT.store(app.get_webview_window("main").is_none(), Ordering::Relaxed);
        // The panel first, so the bar is the last window when it goes.
        for label in [PANEL_LABEL, LABEL] {
            if let Some(window) = app.get_webview_window(label) {
                super::native_window::retire(window);
            }
        }
        *panel() = Panel::default();
        self.covered.store(false, Ordering::Release);
    }
}

/// Builds a hidden window that fades in and out.
fn build(
    app: &AppHandle,
    config: tauri::Result<tauri::utils::config::WindowConfig>,
) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::from_config(app, &config?)?.build()?;
    super::native_window::hide_window_buttons(&window);
    super::native_window::prepare(&window, super::native_window::Motion::Palette);
    log::info!("float_window:opened label={}", window.label());
    Ok(window)
}

fn downloading(task: &Aria2Task) -> bool {
    task.status == "active" && task.seeder.as_deref() != Some("true")
}

/// In `downloading` mode the bar comes with the first download and leaves a
/// moment after the last one; `busy` also holds it while its panel is open.
/// `quiet` remembers when the bar ran out of downloads.
fn wants_bar(
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

/// Whether the foreground window fills `screen`, as a fullscreen one does; a
/// maximized window leaves the taskbar free.
#[cfg_attr(not(windows), allow(dead_code))]
fn fills(window: Rect, screen: Rect) -> bool {
    window.x <= screen.x
        && window.y <= screen.y
        && window.x + window.width >= screen.x + screen.width
        && window.y + window.height >= screen.y + screen.height
}

/// Whether an application in fullscreen is in front of the bar's screen, or
/// of any screen before the bar exists (#726). Windows reports that one is
/// running; its window must also fill the screen, so a video in fullscreen on
/// another screen leaves the bar in place.
#[cfg(windows)]
fn fullscreen_in_front(app: &AppHandle, bar: Option<&WebviewWindow>) -> bool {
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
    let screens = match bar {
        Some(bar) => bar.current_monitor().ok().flatten().into_iter().collect(),
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

/// Saves a mode picked in the bar's menu or the tray and applies it at once;
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

/// Opaque, without a title bar, on top of other windows, out of the taskbar and
/// on every desktop. macOS rounds and shadows only titled windows, so there
/// the title bar stays, transparent and without its buttons.
fn floating_config(
    app: &AppHandle,
    label: &str,
) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = super::add_window::secondary_window_config(app, label)?;
    config.center = false;
    config.decorations = cfg!(target_os = "macos");
    // GTK keeps a window the user cannot resize at least 200x200 (tauri#6125).
    // Without borders the user cannot resize it anyway.
    config.resizable = cfg!(target_os = "linux");
    config.always_on_top = true;
    config.skip_taskbar = true;
    config.focus = false;
    config.visible_on_all_workspaces = true;
    config.title_bar_style = tauri::TitleBarStyle::Overlay;
    config.hidden_title = true;
    config.traffic_light_position = None;
    super::native_window::with_background(&mut config);
    Ok(config)
}

/// The bar appears without taking the focus from the window in use, at the
/// bottom right corner of the screen under the pointer until the user moves
/// it; the window-state plugin restores where it was left.
fn bar_config(app: &AppHandle) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = floating_config(app, LABEL)?;
    config.width = BAR_WIDTH;
    config.height = BAR_HEIGHT;
    // The page takes drops itself: the native handler on Windows only passes
    // files on and turns links away.
    config.drag_drop_enabled = false;
    if let Some((x, y)) = bar_corner(app) {
        config.x = Some(x);
        config.y = Some(y);
    }
    Ok(config)
}

/// Where the bar first appears, in logical pixels. The windows are built off
/// the main thread, but the work area is read there: on X11 it comes from the
/// X server, and Xlib calls from another thread crash the process (#725).
fn bar_corner(app: &AppHandle) -> Option<(f64, f64)> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let corner = super::add_window::pointer_monitor(&handle).map(|monitor| {
            let area = monitor.work_area();
            let scale = monitor.scale_factor();
            (
                f64::from(area.position.x) / scale + f64::from(area.size.width) / scale
                    - BAR_WIDTH
                    - MARGIN,
                f64::from(area.position.y) / scale + f64::from(area.size.height) / scale
                    - BAR_HEIGHT
                    - MARGIN,
            )
        });
        let _ = sender.send(corner);
    })
    .ok()?;
    receiver.recv_timeout(Duration::from_secs(2)).ok().flatten()
}

/// Puts the bar back where the user left it, kept within its screen, and shows
/// it there: a place saved for a smaller window, or on a screen since
/// rearranged, may hang off it. The window-state plugin would do this on the
/// thread that built the window, reading every screen's work area there; on
/// X11 that crashes the process (#725), so it is done on the main thread.
fn restore_bar_position(app: &AppHandle, window: WebviewWindow) {
    use tauri_plugin_window_state::{StateFlags, WindowExt};
    let restored = app.run_on_main_thread(move || {
        if let Err(error) = window.restore_state(StateFlags::POSITION) {
            log::warn!("float_window:restore-failed error={error}");
        }
        if let Err(error) = keep_on_screen(&window) {
            log::warn!("float_window:place-failed error={error}");
        }
        super::native_window::show_inactive(&window, true);
    });
    if let Err(error) = restored {
        log::warn!("float_window:restore-failed error={error}");
    }
}

/// Moves a window that hangs off its screen back into the work area.
fn keep_on_screen(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let position = window.outer_position()?;
    let size = window.outer_size()?;
    let area = monitor.work_area();
    let (x, y) = within(
        Rect {
            x: f64::from(position.x),
            y: f64::from(position.y),
            width: f64::from(size.width),
            height: f64::from(size.height),
        },
        Rect {
            x: f64::from(area.position.x),
            y: f64::from(area.position.y),
            width: f64::from(area.size.width),
            height: f64::from(area.size.height),
        },
    );
    let at = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    if at != position {
        window.set_position(at)?;
    }
    Ok(())
}

/// The panel is sized by its page and placed beside the bar as it opens.
fn panel_config(app: &AppHandle) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = floating_config(app, PANEL_LABEL)?;
    config.width = 344.0;
    config.height = 240.0;
    Ok(config)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Where `window` lands when moved the least to lie within `area`.
fn within(window: Rect, area: Rect) -> (f64, f64) {
    let clamp = |value: f64, length: f64, start: f64, span: f64| {
        value.min(start + span - length).max(start)
    };
    (
        clamp(window.x, window.width, area.x, area.width),
        clamp(window.y, window.height, area.y, area.height),
    )
}

/// Places a panel of `width` × `height` beside the bar: towards the larger
/// half of the screen, centered on the bar and kept on screen. All values in
/// physical pixels; `scale` converts the logical gap.
fn place_panel(bar: Rect, width: f64, height: f64, area: Rect, scale: f64) -> (f64, f64) {
    let gap = PANEL_GAP * scale;
    let x = bar.x + (bar.width - width) / 2.0;
    let above_y = bar.y - gap - height;
    let below_y = bar.y + bar.height + gap;
    let room_above = above_y >= area.y;
    let room_below = below_y + height <= area.y + area.height;
    let lower_half = bar.y + bar.height / 2.0 > area.y + area.height / 2.0;
    let above = (lower_half && room_above) || !room_below;
    let y = if above { above_y } else { below_y };
    within(
        Rect {
            x,
            y,
            width,
            height,
        },
        area,
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
    // A fresh window each time: one shown again after hiding repaints its page
    // only after the system has animated it in. The page presents itself once
    // it has its snapshot. Building waits on the event loop, so not here, and
    // only once the last panel, still fading out, is gone.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        super::native_window::wait_gone(&app, PANEL_LABEL);
        open_panel(&app);
    });
}

fn open_panel(app: &AppHandle) {
    match build(app, panel_config(app)) {
        // Closed again while it was being built.
        Ok(window) if !panel().open => {
            let _ = window.destroy();
        }
        Ok(_) => {}
        Err(error) => {
            log::warn!("float_window:open-failed label={PANEL_LABEL} error={error}");
            panel().open = false;
        }
    }
}

/// Closes the panel and destroys its window as it animates out.
pub fn close_panel(app: &AppHandle) {
    let mut state = panel();
    if !state.open {
        return;
    }
    state.open = false;
    state.closed_at = Some(Instant::now());
    state.bar_offset = None;
    drop(state);
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        super::native_window::retire(window);
    }
}

/// Sizes the panel to its content, given in logical pixels, places it beside
/// the bar and shows it. Main thread only: on X11 the work area it reads
/// comes from the X server (#725).
pub fn present_panel(app: &AppHandle, width: f64, height: f64) -> Result<(), AppError> {
    let missing = |label: &str| AppError::NotFound(format!("{label} window"));
    let failed = |error: tauri::Error| AppError::Io(format!("float panel: {error}"));
    let bar = app
        .get_webview_window(LABEL)
        .ok_or_else(|| missing(LABEL))?;
    let window = app
        .get_webview_window(PANEL_LABEL)
        .ok_or_else(|| missing(PANEL_LABEL))?;
    if !panel().open {
        return Err(AppError::NotFound("open float panel".into()));
    }
    let position = bar.outer_position().map_err(failed)?;
    let size = bar.outer_size().map_err(failed)?;
    let monitor = bar
        .current_monitor()
        .map_err(failed)?
        .ok_or_else(|| missing("float monitor"))?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let bar_rect = Rect {
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
    let (x, y) = place_panel(bar_rect, panel_width, panel_height, area_rect, scale);
    let at = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    panel().bar_offset = Some((position.x - at.x, position.y - at.y));
    window
        .set_size(PhysicalSize::new(panel_width as u32, panel_height as u32))
        .map_err(failed)?;
    window.set_position(at).map_err(failed)?;
    super::native_window::present(&window).map_err(failed)?;
    Ok(())
}

/// Dragging the open panel by its header takes the bar along.
pub fn panel_moved(app: &AppHandle, position: PhysicalPosition<i32>) {
    let Some((dx, dy)) = panel().bar_offset else {
        return;
    };
    if let Some(bar) = app.get_webview_window(LABEL) {
        let _ = bar.set_position(PhysicalPosition::new(position.x + dx, position.y + dy));
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
    use super::{fills, place_panel, queue, wants_bar, within, Mode, Rect, LINGER};
    use crate::aria2::types::Aria2Task;
    use std::time::{Duration, Instant};

    #[test]
    fn only_a_window_filling_the_screen_hides_the_bar() {
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
    fn while_downloading_the_bar_leaves_a_moment_after_the_last_download() {
        let start = Instant::now();
        let mut quiet = None;
        let downloading = Mode::Downloading;
        assert!(!wants_bar(downloading, false, false, &mut quiet, start));
        assert!(wants_bar(downloading, true, false, &mut quiet, start));
        assert!(wants_bar(downloading, true, true, &mut quiet, start));
        // The last download stopped: the bar stays to show its check.
        assert!(wants_bar(downloading, false, true, &mut quiet, start));
        let later = start + LINGER - Duration::from_millis(1);
        assert!(wants_bar(downloading, false, true, &mut quiet, later));
        let expired = start + LINGER;
        assert!(!wants_bar(downloading, false, true, &mut quiet, expired));
        // Gone; the next download brings it back and restarts the wait.
        assert!(!wants_bar(downloading, false, false, &mut quiet, expired));
        assert_eq!(quiet, None);
        assert!(wants_bar(Mode::Always, false, false, &mut quiet, start));
        assert!(!wants_bar(Mode::Off, true, true, &mut quiet, start));
    }

    #[test]
    fn modes_read_their_preference_values_and_unknown_ones_turn_the_bar_off() {
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

    fn bar(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 132.0,
            height: 36.0,
        }
    }

    #[test]
    fn the_panel_opens_centered_on_the_bar_towards_the_larger_part_of_the_screen() {
        // Middle of the lower half: above the bar, centered on it.
        let middle = bar(904.0, 700.0);
        let (x, y) = place_panel(middle, 344.0, 300.0, AREA, 1.0);
        assert_eq!(x + 344.0 / 2.0, middle.x + 132.0 / 2.0);
        assert_eq!(y + 300.0 + 8.0, middle.y);

        // Top left corner: below the bar, kept on screen.
        let corner = bar(16.0, 16.0);
        let (x, y) = place_panel(corner, 344.0, 300.0, AREA, 1.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, corner.y + 36.0 + 8.0);

        // Bottom right corner: above the bar, against the screen's edge.
        let (x, _) = place_panel(bar(1792.0, 988.0), 344.0, 300.0, AREA, 1.0);
        assert_eq!(x + 344.0, AREA.width);
    }

    #[test]
    fn a_panel_without_room_above_opens_below_and_stays_on_screen() {
        let (x, y) = place_panel(bar(1800.0, 300.0), 344.0, 400.0, AREA, 1.0);
        assert_eq!(y, 300.0 + 36.0 + 8.0);
        assert!(x + 344.0 <= AREA.width);
    }

    #[test]
    fn a_bar_saved_off_the_screen_moves_back_the_least() {
        // Saved for the 52 px orb at the right edge: only pulled left.
        assert_eq!(within(bar(1852.0, 972.0), AREA), (1788.0, 972.0));
        assert_eq!(within(bar(-40.0, 1030.0), AREA), (0.0, 1004.0));
        assert_eq!(within(bar(400.0, 300.0), AREA), (400.0, 300.0));
    }
}
