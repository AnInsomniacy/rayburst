#[cfg(not(target_os = "linux"))]
use std::collections::HashMap;
use std::sync::Mutex;
#[cfg(not(target_os = "linux"))]
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri::{AppHandle, Manager, WebviewWindowBuilder};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{setup_tray, TrayMenuState};

/// AppKit uses this alpha channel as a template; other platforms show its original color.
pub const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/64x64.png");

/// Whether the current platform expects the tray icon to be rendered as an
/// AppKit template image.
#[cfg(not(target_os = "linux"))]
pub const TRAY_ICON_IS_TEMPLATE: bool = cfg!(target_os = "macos");

/// Creates a `tauri::image::Image` from the embedded tray icon bytes.
///
/// This is the single source of truth for the tray icon bitmap, shared
/// between initial setup (`setup_tray`) and the `update_tray_title`
/// workaround that must re-set the icon after `set_title` on macOS.
pub fn tray_icon_image() -> tauri::image::Image<'static> {
    tauri::image::Image::from_bytes(TRAY_ICON_BYTES).expect("embedded tray icon is valid PNG")
}

/// Re-applies the tray icon while preserving platform-specific rendering flags.
///
/// macOS menu bar icons must be template images so AppKit can render the same
/// monochrome mask correctly on light, dark, and highlighted menu bar states.
/// Any path that re-sets the icon must restore that flag immediately afterward,
/// otherwise AppKit treats the bitmap as a normal white image.
#[cfg(target_os = "macos")]
pub fn refresh_tray_icon(tray: &tauri::tray::TrayIcon<tauri::Wry>) -> tauri::Result<()> {
    let icon = tray_icon_image();
    tray.set_icon_with_as_template(Some(icon), TRAY_ICON_IS_TEMPLATE)
}

/// Holds references to tray menu items for dynamic label updates (i18n).
/// Used by the `update_tray_menu_labels` command to set localized text
/// at runtime without rebuilding the menu.
#[cfg(not(target_os = "linux"))]
pub struct TrayMenuState {
    pub items: Mutex<HashMap<String, MenuItem<tauri::Wry>>>,
}

/// Serializes window creation without blocking native event callbacks.
#[derive(Default)]
pub struct MainWindowState(pub Mutex<()>);

pub fn request_main_window(app: &AppHandle, source: &'static str, visible: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<MainWindowState>();
        let _creation = state
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let window = get_or_create_main_window(&app);
        let Some((window, created)) = window else {
            log::error!("window:request-failed source={source}");
            return;
        };
        let handle = app.clone();
        // Tauri queues plugin initialization during build(). Restore afterward
        // on the event thread, without a worker holding the plugin's state lock
        // while waiting for window queries on that same thread.
        if let Err(error) = app.run_on_main_thread(move || {
            if created {
                crate::restore_window_state_if_enabled(&handle, &window);
                log::info!("tray:window-recreated label=main");
            }
            if visible {
                activate_main_window(&window, source);
            }
        }) {
            log::error!("window:activation-schedule-failed source={source} error={error}");
        }
    });
}

/// Returns the existing main window, or recreates it if it was destroyed.
///
/// On Linux/Wayland + `decorations: false`, the compositor can destroy
/// the window without emitting `CloseRequested`.  When the user later
/// clicks the tray icon or triggers macOS Reopen, the original window
/// handle is gone.  This function detects that and rebuilds the window
/// from the active platform configuration.
fn get_or_create_main_window(app: &AppHandle) -> Option<(tauri::WebviewWindow, bool)> {
    if let Some(window) = app.get_webview_window("main") {
        return Some((window, false));
    }

    // Window was destroyed — recreate from the same merged platform config
    // that Tauri used for the initial window.
    log::warn!("tray:window-not-found label=main — recreating");
    crate::services::deep_link::mark_frontend_unready(app);
    crate::services::external_input::mark_frontend_unready(app);
    crate::services::frontend_action::mark_frontend_actions_unready(app);

    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "main")
    else {
        log::error!("tray:window-recreate-failed error=main-window-config-not-found");
        return None;
    };

    let builder = match WebviewWindowBuilder::from_config(app, config) {
        Ok(builder) => builder,
        Err(error) => {
            log::error!("tray:window-recreate-failed error={error}");
            return None;
        }
    };

    match builder.build() {
        Ok(w) => Some((w, true)),
        Err(e) => {
            log::error!("tray:window-recreate-failed error={}", e);
            None
        }
    }
}

fn activate_main_window(window: &tauri::WebviewWindow, source: &'static str) {
    log::info!("window:activate-start source={source}");
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        if let Err(e) = window
            .app_handle()
            .set_activation_policy(ActivationPolicy::Regular)
        {
            log::warn!("window:activate-policy-failed source={source} error={e}");
        }
    }

    if let Err(e) = window.unminimize() {
        log::warn!("window:activate-unminimize-failed source={source} error={e}");
    }
    if let Err(e) = window.show() {
        log::warn!("window:activate-show-failed source={source} error={e}");
    }
    if let Err(e) = window.set_focus() {
        log::warn!("window:activate-focus-failed source={source} error={e}");
    }

    log::info!("window:activate-done source={source}");
}

#[cfg(not(target_os = "linux"))]
pub fn setup_tray(app: &AppHandle) -> Result<TrayMenuState, Box<dyn std::error::Error>> {
    // Create MenuItem references for TrayMenuState (used by update_tray_menu_labels).
    // Windows and macOS use Tauri menus; Linux exports the same actions over SNI.
    let show_item = MenuItem::with_id(app, "show", "Show Rayburst", true, None::<&str>)?;
    let new_task_item = MenuItem::with_id(app, "tray-new-task", "New Task", true, None::<&str>)?;
    let resume_all_item =
        MenuItem::with_id(app, "tray-resume-all", "Resume All", true, None::<&str>)?;
    let pause_all_item = MenuItem::with_id(app, "tray-pause-all", "Pause All", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;

    // Clone items before moving into the HashMap — the menu needs the originals,
    // while the HashMap is used for dynamic label updates.
    let mut items_map: HashMap<String, MenuItem<tauri::Wry>> = HashMap::new();
    items_map.insert("show".to_string(), show_item.clone());
    items_map.insert("tray-new-task".to_string(), new_task_item.clone());
    items_map.insert("tray-resume-all".to_string(), resume_all_item.clone());
    items_map.insert("tray-pause-all".to_string(), pause_all_item.clone());
    items_map.insert("tray-quit".to_string(), quit_item.clone());

    // Build the native OS menu — unified for macOS, Windows, and Linux.
    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &PredefinedMenuItem::separator(app)?,
            &new_task_item,
            &resume_all_item,
            &pause_all_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    let _tray = TrayIconBuilder::with_id("rayburst")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Rayburst")
        .icon(tray_icon_image())
        .icon_as_template(TRAY_ICON_IS_TEMPLATE)
        .on_tray_icon_event(|tray, event| {
            // Left-click: show main window (macOS and Windows).
            // Linux libappindicator does not emit TrayIconEvent::Click —
            // the "Show" menu item serves as the equivalent on Linux.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                log::info!("tray:left-click — showing main window");
                request_main_window(app, "tray-left-click", true);
            }
        })
        .on_menu_event(|app, event| {
            dispatch_menu_action(app, event.id.as_ref());
        })
        .build(app)?;

    Ok(TrayMenuState {
        items: Mutex::new(items_map),
    })
}

pub fn dispatch_menu_action(app: &AppHandle, id: &str) {
    match id {
        "show" => {
            log::info!("tray:menu-show — showing main window");
            request_main_window(app, "tray-menu-show", true);
        }
        "tray-pause-all" => {
            log::info!("tray:pause-all — calling aria2 directly");
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(aria2) = app.try_state::<crate::services::tasks::TaskServiceState>() {
                    if let Err(e) = aria2.0.force_pause_all().await {
                        log::warn!("tray:pause-all failed: {e}");
                    }
                }
            });
        }
        "tray-resume-all" => {
            log::info!("tray:resume-all — calling aria2 directly");
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(aria2) = app.try_state::<crate::services::tasks::TaskServiceState>() {
                    match aria2.0.resume_eligible().await {
                        Ok(result) => log::info!(
                            "tray:resume-all resumed={} blocked={}",
                            result.resumed,
                            result.blocked
                        ),
                        Err(e) => log::warn!("tray:resume-all failed: {e}"),
                    }
                }
            });
        }
        "tray-quit" => {
            // Handle quit directly — do NOT emit to frontend.
            // In lightweight mode the WebView is destroyed (window.destroy()),
            // so app.emit() would silently fail. app.exit(0) triggers the
            // RunEvent::Exit handler for full cleanup (save session,
            // stop engine, unmap UPnP). issue #194.
            log::info!("tray:quit — exiting app");
            app.exit(0);
        }
        "tray-new-task" => {
            log::info!("tray:new-task — dispatching frontend action");
            crate::services::frontend_action::dispatch_frontend_action(
                app,
                crate::services::frontend_action::FrontendActionChannel::TrayMenuAction,
                crate::services::frontend_action::FrontendActionKind::NewTask,
                "tray-new-task",
            );
        }
        _ => {}
    }
}

pub fn set_title(app: &AppHandle, title: &str) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(state) = app.try_state::<TrayMenuState>() {
        state.set_title(title);
    }
    #[cfg(not(target_os = "linux"))]
    if let Some(tray) = app.tray_by_id("rayburst") {
        tray.set_title(Some(title))?;
        #[cfg(target_os = "macos")]
        refresh_tray_icon(&tray)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the embedded tray icon bytes are a valid PNG with correct header.
    #[test]
    fn tray_icon_bytes_are_valid_png() {
        // PNG files start with the 8-byte magic signature.
        let png_signature: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(
            TRAY_ICON_BYTES.len() > 8,
            "tray icon file is too small to be a valid PNG"
        );
        assert_eq!(
            &TRAY_ICON_BYTES[..8],
            &png_signature,
            "tray icon does not have valid PNG header"
        );
    }

    /// Verify tray_icon_image() does not panic (bytes decode successfully).
    #[test]
    fn tray_icon_image_does_not_panic() {
        let img = tray_icon_image();
        // Image must have non-zero dimensions.
        assert!(
            !img.rgba().is_empty(),
            "decoded tray icon has no pixel data"
        );
    }
}
