//! The standalone add-task window.
//!
//! Every new download is confirmed in one small, movable window, the way
//! download managers confirm a browser click: links, torrent files, browser
//! handoffs and manual entry alike. It opens over the main window while that is
//! in view, and on the screen under the pointer otherwise. The main window only
//! shows its own add dialog when this window cannot be created.
//!
//! While the main window exists, the add window is built ahead of time and only
//! hidden between downloads, so it appears without loading a new webview.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use super::external_input::ExternalDownloadInput;
use crate::error::AppError;

pub const LABEL: &str = "add";
const INPUT_EVENT: &str = "add-window:inputs";
const FALLBACK_EVENT: &str = "add-window:fallback";
/// How long a new window may take to load before its inputs go to the main
/// window instead.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// The window opened while the main window was destroyed, so the process
    /// lives on in the tray once it closes.
    resident: bool,
    /// Counts openings, so a readiness check only judges its own window.
    generation: u64,
    /// The window could not be created or never loaded; the main window shows
    /// its own dialog for the rest of the session.
    broken: bool,
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
    let Some(state) = app.try_state::<AddWindowState>() else {
        return false;
    };
    if state.lock().broken {
        return false;
    }
    app.get_webview_window(LABEL)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
        || app
            .try_state::<crate::engine::supervisor::EngineSupervisor>()
            .is_some_and(|supervisor| supervisor.is_running())
}

/// Builds the window ahead of its first use, hidden, so it appears at once.
pub fn prepare(app: &AppHandle) {
    let Some(state) = app.try_state::<AddWindowState>() else {
        return;
    };
    let mut inner = state.lock();
    if inner.broken || inner.opening || app.get_webview_window(LABEL).is_some() {
        return;
    }
    inner.opening = true;
    inner.resident = false;
    drop(inner);
    open(app, "prepare");
}

/// Shows the window, placed for this download, once its content is sized.
pub fn present(app: &AppHandle) -> Result<(), AppError> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| AppError::NotFound("add window".into()))?;
    if !window.is_visible().unwrap_or(false) {
        place(app, &window);
    }
    let failed = |error: tauri::Error| AppError::Io(format!("add window: {error}"));
    window.show().map_err(failed)?;
    let _ = window.unminimize();
    window.set_focus().map_err(failed)?;
    Ok(())
}

/// Puts the window away after a download was confirmed or dismissed. It stays
/// built, hidden, while the main window exists, and returns inputs that arrived
/// while it was closing; without the main window it is destroyed, so the
/// process keeps no webview it does not show.
pub fn park(app: &AppHandle) -> AddWindowInputs {
    let Some(window) = app.get_webview_window(LABEL) else {
        return AddWindowInputs::default();
    };
    let Some(state) = app.try_state::<AddWindowState>() else {
        return AddWindowInputs::default();
    };
    if app.get_webview_window("main").is_some() && window.hide().is_ok() {
        let mut inner = state.lock();
        inner.ready = true;
        inner.resident = false;
        return std::mem::take(&mut inner.queue);
    }
    if let Err(error) = window.destroy() {
        log::warn!("add_window:destroy-failed error={error}");
    }
    AddWindowInputs::default()
}

/// Releases a hidden window together with the main window, so it neither holds
/// memory in the tray nor keeps the process alive as its last window.
pub fn main_window_destroyed(app: &AppHandle) {
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

/// Takes inputs the main window forwards; fails when the window cannot be
/// shown, so the main window keeps them.
pub fn forward(app: &AppHandle, batch: AddWindowInputs) -> Result<(), AppError> {
    let broken = app
        .try_state::<AddWindowState>()
        .is_none_or(|state| state.lock().broken);
    if broken {
        return Err(AppError::Conflict("The add window is unavailable".into()));
    }
    route(app, batch, "main-window");
    Ok(())
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

/// Clears the window's state once it is gone, and reopens it for inputs that
/// arrived while it was closing.
pub fn destroyed(app: &AppHandle) {
    let Some(state) = app.try_state::<AddWindowState>() else {
        return;
    };
    let mut inner = state.lock();
    inner.ready = false;
    inner.opening = false;
    inner.resident = inner.resident && app.get_webview_window("main").is_none();
    if !inner.queue.is_empty() {
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
            Ok(_) => {
                log::info!("add_window:opened source={source}");
                tauri::async_runtime::spawn(expect_ready(app, generation, source));
            }
            Err(error) => {
                log::error!("add_window:open-failed source={source} error={error}");
                if let Some(queue) = app.try_state::<AddWindowState>().map(|state| {
                    let mut inner = state.lock();
                    inner.opening = false;
                    inner.broken = true;
                    std::mem::take(&mut inner.queue)
                }) {
                    // Fall back to the main window rather than drop the downloads.
                    hand_to_main_window(&app, queue);
                }
            }
        }
    });
}

/// The add window starts from the main window's configuration. WebView2 refuses
/// a second webview whose environment (browser arguments, proxy, extensions)
/// differs from the first one in the same data directory, so both windows must
/// share those settings. It keeps the main window's title bar style too; its
/// height follows its content, so the user does not resize it.
fn window_config(app: &AppHandle) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "main")
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    config.label = LABEL.to_owned();
    config.url = WebviewUrl::default();
    config.title = "Rayburst".to_owned();
    config.width = 600.0;
    config.height = 520.0;
    config.min_width = None;
    config.min_height = None;
    config.x = None;
    config.y = None;
    config.center = true;
    config.visible = false;
    config.transparent = false;
    config.resizable = false;
    config.maximizable = false;
    Ok(config)
}

/// Centers the window over the main window while that is in view, and on the
/// work area of the screen under the pointer otherwise, where the user just
/// clicked the download.
fn place(app: &AppHandle, window: &WebviewWindow) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let main = app
        .get_webview_window("main")
        .filter(|main| main.is_visible().unwrap_or(false) && !main.is_minimized().unwrap_or(true));
    let anchor = main
        .and_then(|main| Some((main.outer_position().ok()?, main.outer_size().ok()?)))
        .or_else(|| {
            let monitor = app
                .cursor_position()
                .ok()
                .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
                .or_else(|| app.primary_monitor().ok().flatten())?;
            let area = monitor.work_area();
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
/// error. Its inputs then go to the main window instead of waiting there.
async fn expect_ready(app: AppHandle, generation: u64, source: &'static str) {
    tokio::time::sleep(READY_TIMEOUT).await;
    let Some(queue) = app
        .try_state::<AddWindowState>()
        .and_then(|state| abandon(&state, generation))
    else {
        return;
    };
    log::error!(
        "add_window:not-ready source={source} inputs={} links={} items={}",
        queue.inputs.len(),
        queue.links.len(),
        queue.items.len()
    );
    hand_to_main_window(&app, queue);
    if let Some(window) = app.get_webview_window(LABEL) {
        if let Err(error) = window.destroy() {
            log::warn!("add_window:destroy-failed error={error}");
        }
    }
}

/// Gives up on the window of this opening if its frontend never attached, and
/// returns the inputs it was holding.
fn abandon(state: &AddWindowState, generation: u64) -> Option<AddWindowInputs> {
    let mut inner = state.lock();
    if inner.generation != generation || !inner.opening || inner.ready {
        return None;
    }
    inner.opening = false;
    inner.broken = true;
    Some(std::mem::take(&mut inner.queue))
}

fn hand_to_main_window(app: &AppHandle, queue: AddWindowInputs) {
    if !queue.inputs.is_empty() {
        super::external_input::route_to_main_window(
            app,
            queue.inputs,
            "add-window-fallback",
            false,
        );
    }
    if !queue.links.is_empty() {
        super::deep_link::route_to_main_window(app, queue.links, "add-window-fallback", false);
    }
    if !queue.items.is_empty() || queue.manual {
        let rest = AddWindowInputs {
            items: queue.items,
            manual: queue.manual,
            ..AddWindowInputs::default()
        };
        crate::tray::request_main_window(app, "add-window-fallback", true);
        if let Err(error) = app.emit_to("main", FALLBACK_EVENT, &rest) {
            log::error!("add_window:fallback-failed error={error}");
        }
    }
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
    fn a_window_that_never_loads_gives_up_its_own_inputs_only() {
        let state = AddWindowState::default();
        {
            let mut inner = state.lock();
            inner.generation = 2;
            inner.opening = true;
            inner.queue.append(links(&["magnet:?xt=d"]));
        }
        assert!(abandon(&state, 1).is_none());
        assert_eq!(abandon(&state, 2), Some(links(&["magnet:?xt=d"])));
        assert!(!state.lock().opening);
        assert!(state.lock().broken);

        state.lock().opening = true;
        attach(&state);
        assert!(abandon(&state, 2).is_none());
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
