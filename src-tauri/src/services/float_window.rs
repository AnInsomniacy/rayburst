//! The floating window (#669, #718, #402).
//!
//! A small orb that stays on top with the overall speed and progress, so
//! downloads can be followed without the main window. A click opens a panel
//! beside it to follow and control each download. While the preference is on,
//! the pump keeps both open and feeds them each snapshot.
//!
//! The orb never changes its size, and the panel is a window of its own, built
//! hidden with the orb; neither has to move or resize the other to open, so
//! nothing jumps.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow,
    WebviewWindowBuilder,
};

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
/// Size of the orb window in logical pixels: the 44 px orb and the margin
/// holding its shadow, `FloatWindow.vue`.
const ORB_SIZE: f64 = 60.0;
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
}

impl FloatWindow {
    /// `tasks` holds the queued tasks of the same snapshot as `stat`.
    pub fn sync(&self, app: &AppHandle, wanted: bool, stat: &StatUpdate, tasks: &[Aria2Task]) {
        match (wanted, app.get_webview_window(LABEL)) {
            (true, Some(_)) if !self.leaving.load(Ordering::Acquire) => {
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
            (true, None) => self.open(app),
            (false, Some(window)) => self.leave(app, window),
            _ => {}
        }
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
            leaving.store(false, Ordering::Release);
        });
    }
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
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        config.x = Some(
            f64::from(area.position.x) / scale + f64::from(area.size.width) / scale
                - ORB_SIZE
                - MARGIN,
        );
        config.y = Some(
            f64::from(area.position.y) / scale + f64::from(area.size.height) / scale
                - ORB_SIZE
                - MARGIN,
        );
    }
    Ok(config)
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
    /// The panel's right edge lines up with the orb's.
    pub end: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Places a panel of `width` × `height` beside the orb: towards the larger
/// half of the screen, lined up with the orb's outer edge and kept on screen.
/// All values in physical pixels; `scale` converts the logical insets.
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
    let end = orb.x + orb.width / 2.0 > area.x + area.width / 2.0;
    let x = if end {
        orb.x + orb.width - orb_inset + panel_inset - width
    } else {
        orb.x + orb_inset - panel_inset
    };
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
        PanelSide { above, end },
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
/// the orb and shows it.
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
    use super::{place_panel, queue, PanelSide, Rect};
    use crate::aria2::types::Aria2Task;

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
            width: 60.0,
            height: 60.0,
        }
    }

    #[test]
    fn the_panel_opens_towards_the_larger_part_of_the_screen() {
        // Bottom right corner: above the orb, drawn right edges lined up.
        let corner = orb(1844.0, 964.0);
        let (x, y, side) = place_panel(corner, 344.0, 300.0, AREA, 1.0);
        assert_eq!(
            side,
            PanelSide {
                above: true,
                end: true
            }
        );
        assert_eq!(x + 344.0 - 12.0, corner.x + 60.0 - 8.0);
        assert_eq!(y + 300.0 - 12.0 + 8.0, corner.y + 8.0);

        // Top left corner: below the orb, drawn left edges lined up.
        let corner = orb(16.0, 16.0);
        let (x, y, side) = place_panel(corner, 344.0, 300.0, AREA, 1.0);
        assert_eq!(
            side,
            PanelSide {
                above: false,
                end: false
            }
        );
        assert_eq!(x + 12.0, corner.x + 8.0);
        assert_eq!(y + 12.0, corner.y + 60.0 - 8.0 + 8.0);
    }

    #[test]
    fn a_panel_without_room_above_opens_below_and_stays_on_screen() {
        let (x, y, side) = place_panel(orb(1800.0, 300.0), 344.0, 400.0, AREA, 1.0);
        assert!(!side.above);
        assert!(x + 344.0 - 12.0 <= AREA.width);
        assert!(y + 12.0 >= 0.0);
    }
}
