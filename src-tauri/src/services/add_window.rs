//! The standalone add-task window.
//!
//! Downloads handed over while the main window is out of sight open a small
//! confirmation window instead of raising the whole application, the way
//! download managers confirm a browser click. The main window keeps handling
//! inputs while the user is looking at it, and during startup, before the
//! engine it would submit to is running.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::external_input::ExternalDownloadInput;

pub const LABEL: &str = "add";
const INPUT_EVENT: &str = "add-window:inputs";

/// Inputs for the add window: structured browser handoffs and plain links.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddWindowInputs {
    pub inputs: Vec<ExternalDownloadInput>,
    pub links: Vec<String>,
}

impl AddWindowInputs {
    fn is_empty(&self) -> bool {
        self.inputs.is_empty() && self.links.is_empty()
    }

    fn append(&mut self, other: AddWindowInputs) {
        self.inputs.extend(other.inputs);
        for link in other.links {
            if !self.links.contains(&link) {
                self.links.push(link);
            }
        }
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

/// Whether new inputs belong in the add window rather than the main window.
pub fn wanted(app: &AppHandle) -> bool {
    if app.get_webview_window(LABEL).is_some() {
        return true;
    }
    let engine_running = app
        .try_state::<crate::engine::supervisor::EngineSupervisor>()
        .is_some_and(|supervisor| supervisor.is_running());
    if !engine_running {
        return false;
    }
    match app.get_webview_window("main") {
        Some(window) => {
            !window.is_visible().unwrap_or(true) || window.is_minimized().unwrap_or(false)
        }
        None => true,
    }
}

/// Hands inputs to the add window, opening it when needed.
pub fn route(app: &AppHandle, batch: AddWindowInputs, source: &'static str) {
    let Some(state) = app.try_state::<AddWindowState>() else {
        log::error!("add_window:state-unavailable source={source}");
        return;
    };
    let mut inner = state.lock();
    log::info!(
        "add_window:route source={source} inputs={} links={} ready={} opening={}",
        batch.inputs.len(),
        batch.links.len(),
        inner.ready,
        inner.opening
    );
    if inner.ready {
        match app.emit_to(LABEL, INPUT_EVENT, &batch) {
            Ok(()) => return,
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
    // Building a window waits on the event loop; never do it on that thread.
    tauri::async_runtime::spawn_blocking(move || {
        let built = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::default())
            .title("Rayburst")
            .inner_size(600.0, 640.0)
            .min_inner_size(420.0, 460.0)
            .center()
            .visible(false)
            .build();
        match built {
            Ok(_) => log::info!("add_window:opened source={source}"),
            Err(error) => {
                log::error!("add_window:open-failed source={source} error={error}");
                if let Some(state) = app.try_state::<AddWindowState>() {
                    let mut inner = state.lock();
                    inner.opening = false;
                    let queue = std::mem::take(&mut inner.queue);
                    drop(inner);
                    // Fall back to the main window rather than drop the downloads.
                    hand_to_main_window(&app, queue);
                }
            }
        }
    });
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
}

#[cfg(test)]
mod tests {
    use super::{attach, detach, AddWindowInputs, AddWindowState};

    fn links(values: &[&str]) -> AddWindowInputs {
        AddWindowInputs {
            inputs: Vec::new(),
            links: values.iter().map(ToString::to_string).collect(),
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
}
