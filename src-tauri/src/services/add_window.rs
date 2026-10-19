//! The standalone add-task window.
//!
//! Every new download is confirmed in one small, movable window, the way
//! download managers confirm a browser click: links, torrent files, browser
//! handoffs and manual entry alike. It opens over the main window while that is
//! in view, and on the screen under the pointer otherwise.
//!
//! It is built, hidden, once the app has started and lives until the app exits:
//! closing it only hides it, so a download never waits for a webview to load.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use super::external_input::ExternalDownloadInput;
use crate::error::AppError;

#[cfg(target_os = "macos")]
mod motion;

pub const LABEL: &str = "add";
const INPUT_EVENT: &str = "add-window:inputs";
/// How long a new window may take to load before it is given up.
const READY_TIMEOUT: Duration = Duration::from_secs(10);
/// Transparent margin around the panel the window draws, which holds the
/// panel's shadow. Matches `PANEL_INSET` in `AddWindow.vue`. macOS frames the
/// window itself, around its traffic lights, so there is none there.
const PANEL_INSET: f64 = if cfg!(target_os = "macos") { 0.0 } else { 16.0 };

/// Inputs for the add window: structured browser handoffs, plain links and
/// torrent paths, batch items prepared by the main window, and a request for
/// an empty dialog.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AddWindowInputs {
    pub inputs: Vec<ExternalDownloadInput>,
    pub links: Vec<String>,
    /// Batch items built by the main window; their shape belongs to the frontend.
    pub items: Vec<serde_json::Value>,
    /// Opens the dialog even when there is nothing to add yet.
    pub manual: bool,
}

impl AddWindowInputs {
    fn is_empty(&self) -> bool {
        self.inputs.is_empty() && self.links.is_empty() && self.items.is_empty() && !self.manual
    }

    fn append(&mut self, other: AddWindowInputs) {
        self.inputs.extend(other.inputs);
        for link in other.links {
            if !self.links.contains(&link) {
                self.links.push(link);
            }
        }
        self.items.extend(other.items);
        self.manual |= other.manual;
    }
}

#[derive(Debug, Default)]
struct Inner {
    queue: AddWindowInputs,
    /// The window is being built; its frontend has not taken the queue yet.
    opening: bool,
    /// The frontend listens for inputs.
    ready: bool,
    /// The process lives in the tray without the main window, so losing the
    /// add window as the last window must not end it.
    resident: bool,
    /// The main window is being released into the tray (lightweight mode)
    /// rather than lost.
    releasing_main: bool,
    /// Counts openings, so a readiness check only judges its own window.
    generation: u64,
    /// The last window could not be created or never loaded; its inputs wait
    /// for the next request instead of reopening it at once.
    stalled: bool,
}

#[derive(Default)]
pub struct AddWindowState(Mutex<Inner>);

impl AddWindowState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Whether native inputs belong in the add window rather than the main window.
/// During startup, before the engine it would submit to is running, the main
/// window receives them and forwards them once it is ready.
pub fn wanted(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
        || app
            .try_state::<crate::engine::supervisor::EngineSupervisor>()
            .is_some_and(|supervisor| supervisor.is_running())
}

/// Builds the window ahead of its first use, hidden, so it appears at once.
/// Only beside the main window: in the tray, lightweight mode keeps no
/// webview, so the add window is built when a download arrives (#735).
pub fn prepare(app: &AppHandle) {
    let Some(state) = app.try_state::<AddWindowState>() else {
        return;
    };
    let mut inner = state.lock();
    if inner.stalled
        || inner.opening
        || app.get_webview_window(LABEL).is_some()
        || app.get_webview_window("main").is_none()
    {
        return;
    }
    inner.opening = true;
    drop(inner);
    open(app, "prepare");
}

/// Shows the window, placed for this download, once its content is sized.
/// On macOS the window itself rises in unless `animate` is off; elsewhere the
/// page animates its panel. Main thread only: placing it reads work areas,
/// see [`screen_height`].
pub fn present(app: &AppHandle, animate: bool) -> Result<(), AppError> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| AppError::NotFound("add window".into()))?;
    let hidden = !window.is_visible().unwrap_or(false);
    if hidden {
        place(app, &window);
    }
    #[cfg(target_os = "macos")]
    let entrance = hidden.then(|| motion::stage(&window, animate)).flatten();
    #[cfg(not(target_os = "macos"))]
    let _ = animate;
    let failed = |error: tauri::Error| AppError::Io(format!("add window: {error}"));
    window.show().map_err(failed)?;
    let _ = window.unminimize();
    window.set_focus().map_err(failed)?;
    #[cfg(target_os = "macos")]
    if let Some(entrance) = entrance {
        entrance.run();
    }
    Ok(())
}

/// Gives the window's content `width` × `height` logical pixels, keeping its
/// top edge. On macOS the window frames its content without a margin and
/// follows it frame by frame, together with its entrance (`motion.rs`).
pub fn fit(app: &AppHandle, width: f64, height: f64) -> Result<(), AppError> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| AppError::NotFound("add window".into()))?;
    #[cfg(target_os = "macos")]
    motion::fit(&window, width, height);
    #[cfg(not(target_os = "macos"))]
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|error| AppError::Io(format!("add window: {error}")))?;
    Ok(())
}

/// Lets the window move away before it is put away: on macOS the window
/// fades out as it sinks, and this returns once it has. Elsewhere the page
/// animates its panel and this returns at once.
pub async fn leave(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        let (done, finished) = tokio::sync::oneshot::channel();
        let dispatched = app.run_on_main_thread(move || {
            motion::leave(&window, move || {
                let _ = done.send(());
            });
        });
        match dispatched {
            Ok(()) => {
                let _ = finished.await;
            }
            Err(error) => log::warn!("add_window:leave-failed error={error}"),
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Hands inputs to the add window, opening it when needed.
pub fn route(app: &AppHandle, batch: AddWindowInputs, source: &'static str) {
    let Some(state) = app.try_state::<AddWindowState>() else {
        log::error!("add_window:state-unavailable source={source}");
        return;
    };
    let mut inner = state.lock();
    log::info!(
        "add_window:route source={source} inputs={} links={} items={} manual={} ready={} opening={}",
        batch.inputs.len(),
        batch.links.len(),
        batch.items.len(),
        batch.manual,
        inner.ready,
        inner.opening
    );
    if inner.ready {
        match app.emit_to(LABEL, INPUT_EVENT, &batch) {
            Ok(()) => {
                drop(inner);
                raise(app);
                return;
            }
            Err(error) => {
                log::warn!("add_window:emit-failed source={source} error={error}");
                inner.ready = false;
            }
        }
    }
    inner.queue.append(batch);
    inner.stalled = false;
    if !inner.opening && app.get_webview_window(LABEL).is_none() {
        inner.opening = true;
        inner.resident = app.get_webview_window("main").is_none();
        drop(inner);
        open(app, source);
    }
}

/// Brings an open add window in front of the browser that sent more inputs.
fn raise(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let _ = window.unminimize();
    if let Err(error) = window.set_focus() {
        log::warn!("add_window:focus-failed error={error}");
    }
}

/// Marks the frontend ready and returns what arrived before it listened.
pub fn attach(state: &AddWindowState) -> AddWindowInputs {
    let mut inner = state.lock();
    inner.ready = true;
    inner.opening = false;
    std::mem::take(&mut inner.queue)
}

/// Stops delivery before the frontend closes the window; returns inputs that
/// arrived meanwhile, which keep the window open.
pub fn detach(state: &AddWindowState) -> AddWindowInputs {
    let mut inner = state.lock();
    let queue = std::mem::take(&mut inner.queue);
    inner.ready = inner.ready && !queue.is_empty();
    queue
}

/// Hides the window after a download was confirmed or dismissed, and returns
/// inputs that arrived while it was closing. Without the main window it is
/// destroyed instead: in the tray no webview stays behind (#735), and an app
/// that lost its main window ends as it would without it.
pub fn park(app: &AppHandle) -> AddWindowInputs {
    let (Some(window), Some(state)) = (
        app.get_webview_window(LABEL),
        app.try_state::<AddWindowState>(),
    ) else {
        return AddWindowInputs::default();
    };
    if app.get_webview_window("main").is_some() {
        match window.hide() {
            Ok(()) => {
                let mut inner = state.lock();
                inner.ready = true;
                return std::mem::take(&mut inner.queue);
            }
            Err(error) => log::warn!("add_window:hide-failed error={error}"),
        }
    }
    if let Err(error) = window.destroy() {
        log::warn!("add_window:destroy-failed error={error}");
    }
    AddWindowInputs::default()
}

/// Marks the coming destruction of the main window as a release into the tray.
pub fn main_window_releasing(app: &AppHandle) {
    if let Some(state) = app.try_state::<AddWindowState>() {
        state.lock().releasing_main = true;
    }
}

/// The hidden add window goes with the main window. Released into the tray,
/// the process lives on without any webview (#735) and builds the add window
/// when a download arrives; lost otherwise (a compositor closing it outright),
/// the app ends or stays exactly as it would without it. A window in use
/// stays until it is closed.
pub fn main_window_destroyed(app: &AppHandle) {
    let Some(state) = app.try_state::<AddWindowState>() else {
        return;
    };
    let mut inner = state.lock();
    if std::mem::take(&mut inner.releasing_main) {
        inner.resident = true;
    }
    drop(inner);
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        return;
    }
    if let Err(error) = window.destroy() {
        log::warn!("add_window:destroy-failed error={error}");
    }
}

/// Clears the window's state once it is gone, and reopens it for inputs that
/// arrived while it was closing; not for those of a window that failed.
pub fn destroyed(app: &AppHandle) {
    let Some(state) = app.try_state::<AddWindowState>() else {
        return;
    };
    let mut inner = state.lock();
    inner.ready = false;
    inner.opening = false;
    inner.resident = inner.resident && app.get_webview_window("main").is_none();
    if !inner.queue.is_empty() && !inner.stalled {
        inner.opening = true;
        drop(inner);
        open(app, "reopen");
    }
}

/// Whether the last window to close was an add window opened from the tray,
/// so the implicit exit that follows must not end the process.
pub fn keeps_process(app: &AppHandle) -> bool {
    app.try_state::<AddWindowState>()
        .is_some_and(|state| state.lock().resident && app.get_webview_window("main").is_none())
}

/// Forgets tray residency once the main window exists again.
pub fn main_window_created(app: &AppHandle) {
    if let Some(state) = app.try_state::<AddWindowState>() {
        state.lock().resident = false;
    }
}

fn open(app: &AppHandle, source: &'static str) {
    let app = app.clone();
    let Some(generation) = app.try_state::<AddWindowState>().map(|state| {
        let mut inner = state.lock();
        inner.generation += 1;
        inner.generation
    }) else {
        return;
    };
    // Building a window waits on the event loop; never do it on that thread.
    tauri::async_runtime::spawn_blocking(move || {
        let built = window_config(&app)
            .and_then(|config| WebviewWindowBuilder::from_config(&app, &config))
            .and_then(WebviewWindowBuilder::build);
        match built {
            Ok(window) => {
                without_system_transitions(&window);
                log::info!("add_window:opened source={source}");
                tauri::async_runtime::spawn(expect_ready(app, generation, source));
            }
            Err(error) => {
                log::error!("add_window:open-failed source={source} error={error}");
                if let Some(state) = app.try_state::<AddWindowState>() {
                    let mut inner = state.lock();
                    inner.opening = false;
                    inner.stalled = true;
                }
            }
        }
    });
}

/// The window is transparent and its page draws the panel, with rounded
/// corners and a shadow, so the panel can animate in and out and grow and
/// shrink smoothly inside a window that is resized only once per change. The
/// native shadow would outline the whole transparent window instead.
///
/// macOS draws its traffic lights in the window, not in the page's panel, and
/// they would not follow the panel's margin, motion or interface size. There
/// the window is framed like the main window: the content fills it, the
/// system rounds and shadows it, and the window itself moves in and out
/// (`motion.rs`). Its traffic lights keep the system position, which AppKit
/// restores whenever it lays out the title bar; a custom one is lost on each
/// showing (tauri#13044) and redrawn only on resize.
fn window_config(app: &AppHandle) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = secondary_window_config(app, LABEL)?;
    config.width = 600.0 + 2.0 * PANEL_INSET;
    config.height = 560.0;
    config.center = true;
    config.transparent = true;
    config.shadow = cfg!(target_os = "macos");
    // GTK ignores programmatic resizes of a window the user cannot resize
    // (tao#545, tauri#6125), and this window follows its content (#731).
    config.resizable = cfg!(target_os = "linux");
    Ok(config)
}

/// Secondary windows start from the main window's configuration. WebView2
/// refuses a second webview whose environment (browser arguments, proxy,
/// extensions) differs from the first one in the same data directory, so all
/// windows must share those settings.
pub(crate) fn secondary_window_config(
    app: &AppHandle,
    label: &str,
) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "main")
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    config.label = label.to_owned();
    config.url = WebviewUrl::default();
    config.title = "Rayburst".to_owned();
    config.min_width = None;
    config.min_height = None;
    config.x = None;
    config.y = None;
    config.visible = false;
    config.resizable = false;
    config.maximizable = false;
    Ok(config)
}

/// Windows fades a window in and out as it is shown and hidden. The page
/// animates its own content instead, which that fade would cover up.
pub(crate) fn without_system_transitions(window: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED,
        };
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let disabled: i32 = 1;
        // SAFETY: the handle belongs to a live window, and the attribute
        // reads a BOOL from a pointer valid for the length passed.
        let result = unsafe {
            DwmSetWindowAttribute(
                hwnd.0 as _,
                DWMWA_TRANSITIONS_FORCEDISABLED as _,
                std::ptr::from_ref(&disabled).cast(),
                std::mem::size_of::<i32>() as u32,
            )
        };
        if result != 0 {
            log::debug!("window:transitions-not-disabled hresult={result:#x}");
        }
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// Centers the window over the main window while that is in view, and on the
/// work area of the screen under the pointer otherwise, where the user just
/// clicked the download.
fn place(app: &AppHandle, window: &WebviewWindow) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let anchor = main_in_view(app)
        .and_then(|main| Some((main.outer_position().ok()?, main.outer_size().ok()?)))
        .or_else(|| {
            let area = *pointer_monitor(app)?.work_area();
            Some((area.position, area.size))
        });
    // Without an anchor (Wayland reports no positions) the window stays centered.
    let Some((origin, extent)) = anchor else {
        return;
    };
    let mut position = centered(origin, extent, size);
    // Keep the whole window on the screen it lands on.
    let center_x = f64::from(position.x) + f64::from(size.width) / 2.0;
    let center_y = f64::from(position.y) + f64::from(size.height) / 2.0;
    if let Some(monitor) = app.monitor_from_point(center_x, center_y).ok().flatten() {
        let area = monitor.work_area();
        position = clamped(position, size, area.position, area.size);
    }
    if let Err(error) = window.set_position(position) {
        log::warn!("add_window:place-failed error={error}");
    }
}

/// Logical height of the work area of the screen the window opens on, which
/// caps its height. Only for synchronous commands, which run on the main
/// thread: on X11 a work area is read from the X server, and Xlib calls from
/// another thread crash the process (#725).
pub fn screen_height(app: &AppHandle) -> Option<f64> {
    let monitor = main_in_view(app)
        .and_then(|main| main.current_monitor().ok().flatten())
        .or_else(|| pointer_monitor(app))?;
    Some(f64::from(monitor.work_area().size.height) / monitor.scale_factor())
}

fn main_in_view(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
        .filter(|main| main.is_visible().unwrap_or(false) && !main.is_minimized().unwrap_or(true))
}

/// The screen under the pointer, where the user just clicked the download.
pub(crate) fn pointer_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    app.cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())
}

fn centered(
    origin: PhysicalPosition<i32>,
    extent: PhysicalSize<u32>,
    size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    let offset = |outer: u32, inner: u32| (i64::from(outer) - i64::from(inner)) / 2;
    PhysicalPosition::new(
        (i64::from(origin.x) + offset(extent.width, size.width)) as i32,
        (i64::from(origin.y) + offset(extent.height, size.height)) as i32,
    )
}

fn clamped(
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    origin: PhysicalPosition<i32>,
    extent: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    let axis = |at: i32, length: u32, start: i32, span: u32| {
        let end = i64::from(start) + i64::from(span) - i64::from(length);
        i64::from(at).min(end).max(i64::from(start)) as i32
    };
    PhysicalPosition::new(
        axis(position.x, size.width, origin.x, extent.width),
        axis(position.y, size.height, origin.y, extent.height),
    )
}

/// A built window may still fail to load its webview without reporting an
/// error. It is then destroyed, and its inputs wait for the next request.
async fn expect_ready(app: AppHandle, generation: u64, source: &'static str) {
    tokio::time::sleep(READY_TIMEOUT).await;
    let abandoned = app
        .try_state::<AddWindowState>()
        .is_some_and(|state| abandon(&state, generation));
    if !abandoned {
        return;
    }
    log::error!("add_window:not-ready source={source}");
    if let Some(window) = app.get_webview_window(LABEL) {
        if let Err(error) = window.destroy() {
            log::warn!("add_window:destroy-failed error={error}");
        }
    }
}

/// Gives up on the window of this opening if its frontend never attached.
fn abandon(state: &AddWindowState, generation: u64) -> bool {
    let mut inner = state.lock();
    if inner.generation != generation || !inner.opening || inner.ready {
        return false;
    }
    inner.opening = false;
    inner.stalled = true;
    true
}

#[cfg(test)]
mod tests {
    use super::{abandon, attach, centered, clamped, detach, AddWindowInputs, AddWindowState};
    use tauri::{PhysicalPosition, PhysicalSize};

    fn links(values: &[&str]) -> AddWindowInputs {
        AddWindowInputs {
            links: values.iter().map(ToString::to_string).collect(),
            ..AddWindowInputs::default()
        }
    }

    #[test]
    fn queued_inputs_reach_the_frontend_once() {
        let state = AddWindowState::default();
        {
            let mut inner = state.lock();
            inner.queue.append(links(&["magnet:?xt=a", "magnet:?xt=b"]));
            inner.queue.append(links(&["magnet:?xt=a"]));
        }
        assert_eq!(attach(&state), links(&["magnet:?xt=a", "magnet:?xt=b"]));
        assert!(attach(&state).is_empty());
    }

    #[test]
    fn inputs_arriving_while_closing_keep_the_window() {
        let state = AddWindowState::default();
        attach(&state);
        assert!(detach(&state).is_empty());
        assert!(!state.lock().ready);

        attach(&state);
        state.lock().queue.append(links(&["magnet:?xt=c"]));
        assert_eq!(detach(&state), links(&["magnet:?xt=c"]));
        assert!(state.lock().ready);
    }

    #[test]
    fn a_window_that_never_loads_keeps_its_inputs_for_the_next_request() {
        let state = AddWindowState::default();
        {
            let mut inner = state.lock();
            inner.generation = 2;
            inner.opening = true;
            inner.queue.append(links(&["magnet:?xt=d"]));
        }
        assert!(!abandon(&state, 1));
        assert!(abandon(&state, 2));
        assert!(!state.lock().opening);
        assert!(state.lock().stalled);
        assert_eq!(attach(&state), links(&["magnet:?xt=d"]));

        state.lock().opening = true;
        assert!(!abandon(&state, 2));
    }

    #[test]
    fn a_request_for_an_empty_dialog_keeps_the_window() {
        let state = AddWindowState::default();
        attach(&state);
        state.lock().queue.append(AddWindowInputs {
            manual: true,
            ..AddWindowInputs::default()
        });
        assert!(detach(&state).manual);
        assert!(state.lock().ready);
    }

    #[test]
    fn the_window_lands_centered_and_inside_the_screen() {
        let window = PhysicalSize::new(600, 640);
        assert_eq!(
            centered(
                PhysicalPosition::new(100, 50),
                PhysicalSize::new(1000, 800),
                window
            ),
            PhysicalPosition::new(300, 130)
        );
        // A main window smaller than the add window near the screen edge.
        let position = centered(
            PhysicalPosition::new(-20, -10),
            PhysicalSize::new(400, 300),
            window,
        );
        assert_eq!(
            clamped(
                position,
                window,
                PhysicalPosition::new(0, 0),
                PhysicalSize::new(1920, 1040)
            ),
            PhysicalPosition::new(0, 0)
        );
        assert_eq!(
            clamped(
                PhysicalPosition::new(1800, 900),
                window,
                PhysicalPosition::new(0, 0),
                PhysicalSize::new(1920, 1040)
            ),
            PhysicalPosition::new(1320, 400)
        );
    }
}
