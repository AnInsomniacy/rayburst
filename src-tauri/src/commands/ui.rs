use crate::error::AppError;
use crate::tray::TrayMenuState;
use serde_json::Value;
#[cfg(not(target_os = "macos"))]
use tauri::window::ProgressBarState;
use tauri::AppHandle;
use tauri::Manager;

/// Updates the system tray title text.
///
/// Supported platforms:
/// - **macOS**: renders in the menu bar next to the tray icon
/// - **Linux**: renders as an appindicator label next to the icon
/// - **Windows**: no-op (Windows system tray has no title API)
#[tauri::command]
pub fn update_tray_title(app: AppHandle, title: String) -> Result<(), AppError> {
    crate::tray::set_title(&app, &title).map_err(|error| AppError::Io(error.to_string()))
}

/// Updates localized labels on tray menu items by their IDs.
#[tauri::command]
pub fn update_tray_menu_labels(app: AppHandle, labels: Value) -> Result<(), AppError> {
    app.state::<TrayMenuState>().set_labels(&labels);
    Ok(())
}

/// Updates localized labels on application menu items by their IDs.
///
/// Recursively traverses all submenus so that items nested inside
/// submenus are found — `Menu::get()` only checks direct children.
#[tauri::command]
pub fn update_menu_labels(app: AppHandle, labels: Value) -> Result<(), AppError> {
    use tauri::menu::MenuItemKind;

    fn apply_labels(items: &[MenuItemKind<tauri::Wry>], map: &serde_json::Map<String, Value>) {
        for item in items {
            match item {
                MenuItemKind::MenuItem(mi) => {
                    if let Some(text) = map.get(mi.id().as_ref()) {
                        let _ = mi.set_text(text.as_str().unwrap_or_default());
                    }
                }
                MenuItemKind::Submenu(sub) => {
                    if let Some(text) = map.get(sub.id().as_ref()) {
                        let _ = sub.set_text(text.as_str().unwrap_or_default());
                    }
                    if let Ok(children) = sub.items() {
                        apply_labels(&children, map);
                    }
                }
                // PredefinedMenuItems have auto-generated UUIDs that cannot
                // be predicted, so we match by their current display text
                // instead (keyed by the English default in the labels map).
                MenuItemKind::Predefined(pi) => {
                    if let Ok(current) = pi.text() {
                        if let Some(new_text) = map.get(&current) {
                            let _ = pi.set_text(new_text.as_str().unwrap_or_default());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(menu) = app.menu() {
        if let Some(obj) = labels.as_object() {
            if let Ok(items) = menu.items() {
                apply_labels(&items, obj);
            }
        }
    }
    Ok(())
}

/// Updates the taskbar/dock progress bar (0.0–1.0 for progress, negative to clear).
///
/// macOS: Uses `set_dock_progress()` (NSDockTile, app-level — no Window required).
/// Windows: Uses `window.set_progress_bar()` (taskbar progress, requires Window).
#[tauri::command]
pub fn update_progress_bar(app: AppHandle, progress: f64) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        if progress < 0.0 {
            crate::services::stat::set_dock_progress(None);
        } else {
            crate::services::stat::set_dock_progress(Some((progress * 100.0) as u64));
        }
    }

    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("main") {
        if progress < 0.0 {
            let _ = window.set_progress_bar(ProgressBarState {
                status: Some(tauri::window::ProgressBarStatus::None),
                progress: None,
            });
        } else {
            let _ = window.set_progress_bar(ProgressBarState {
                status: Some(tauri::window::ProgressBarStatus::Normal),
                progress: Some((progress * 100.0) as u64),
            });
        }
    }

    let _ = app; // suppress unused warning when macOS
    Ok(())
}

/// Updates the macOS dock badge label (empty string clears the badge).
///
/// Uses `set_dock_badge()` which calls `NSApp().dockTile().setBadgeLabel()`
/// directly — no Window object required.  This ensures the badge updates
/// even when the WebView is destroyed in lightweight mode.
#[tauri::command]
pub fn update_dock_badge(_app: AppHandle, label: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        if label.is_empty() {
            crate::services::stat::set_dock_badge(None);
        } else {
            crate::services::stat::set_dock_badge(Some(&label));
        }
    }
    let _ = _app; // suppress unused warning on non-macOS
    let _ = label;
    Ok(())
}

/// Toggles the macOS Dock icon visibility at runtime.
/// When `visible` is false, reads the `hideDockOnMinimize` preference from
/// the persistent store — only hides the Dock icon if the user opted in.
/// When `visible` is true, always restores the icon (e.g. on Reopen / tray show).
/// No-op on non-macOS platforms.
#[tauri::command]
pub fn set_dock_visible(app: AppHandle, visible: bool) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        use tauri_plugin_store::StoreExt;

        if visible {
            let _ = app.set_activation_policy(ActivationPolicy::Regular);
        } else {
            let hide_dock = app
                .store("config.json")
                .ok()
                .and_then(|s| s.get("preferences"))
                .and_then(|p| p.get("hideDockOnMinimize")?.as_bool())
                .unwrap_or(false);
            if hide_dock {
                let _ = app.set_activation_policy(ActivationPolicy::Accessory);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
    Ok(())
}

/// Shows the main window, from the floating window.
#[tauri::command]
pub fn float_window_open_main(app: AppHandle) {
    crate::tray::request_main_window(&app, "float-window", true);
}

/// The floating window listens now; it gets a snapshot without waiting for the
/// next tick.
#[tauri::command]
pub fn float_window_ready(app: AppHandle) {
    crate::services::pump::wake(&app);
}

/// Paints the calling window in its theme's background, which also starts the
/// windows built later.
#[tauri::command]
pub fn window_background(
    window: tauri::WebviewWindow,
    red: u8,
    green: u8,
    blue: u8,
) -> Result<(), AppError> {
    crate::services::native_window::set_background(&window, (red, green, blue))
        .map_err(|error| AppError::Io(format!("window background: {error}")))
}

/// Switches the floating window's mode from its menu.
#[tauri::command]
pub fn float_window_set_mode(
    app: AppHandle,
    mode: crate::services::float_window::Mode,
) -> Result<(), AppError> {
    crate::services::float_window::set_mode(&app, mode)
}

/// Opens the floating panel beside the bar, or closes it.
#[tauri::command]
pub fn float_panel_toggle(app: AppHandle) {
    crate::services::float_window::toggle_panel(&app);
}

/// Closes the floating panel, which lost the focus or whose bar is dragged.
#[tauri::command]
pub fn float_panel_dismiss(app: AppHandle) {
    crate::services::float_window::close_panel(&app);
}

/// Places the floating panel, sized to its content, beside the bar and shows it.
#[tauri::command]
pub fn float_panel_present(app: AppHandle, width: f64, height: f64) -> Result<(), AppError> {
    crate::services::float_window::present_panel(&app, width, height)
}

/// True only for the first main page of the process. Text already on the
/// clipboard then was copied before Rayburst ran, not for it (#738); a page
/// recreated from the tray still picks up links copied meanwhile.
#[tauri::command]
pub fn clipboard_baseline_wanted() -> bool {
    static TAKEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    !TAKEN.swap(true, std::sync::atomic::Ordering::Relaxed)
}

/// Programmatically triggers the minimize-to-tray flow from the frontend.
///
/// Delegates entirely to [`handle_minimize_to_tray()`](crate::handle_minimize_to_tray)
/// — the same code path used by `CloseRequested` and `Cmd+W`.  In lightweight
/// mode this destroys the WebView; in standard mode it hides the window.
///
/// Primary use case: autostart + lightweight mode.  The frontend calls this
/// **after** completing all initialization (engine start, option sync,
/// resume-all) so the WebView can be safely destroyed without breaking
/// the startup sequence.  All background services (stat polling, task
/// monitor, speed scheduler) continue running in Rust.
///
/// Cross-platform: macOS Dock hiding (`hideDockOnMinimize`) and the
/// cold-start phase transition (`end_cold_start`) are handled internally
/// by `handle_minimize_to_tray` — no platform-specific logic needed here.
#[tauri::command]
pub fn minimize_to_tray(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        crate::handle_minimize_to_tray(&app, &window);
    }
}
