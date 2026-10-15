#[cfg(not(target_os = "linux"))]
use std::collections::HashMap;
use std::sync::Mutex;
#[cfg(not(target_os = "linux"))]
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri::{AppHandle, Manager, WebviewWindowBuilder};

use crate::services::float_window::Mode as FloatMode;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{setup_tray, TrayMenuState};

/// AppKit uses this alpha channel as a template; Linux panels scale it smoothly.
#[cfg(any(not(windows), test))]
pub const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/64x64.png");

/// The Windows shell shrinks a larger icon with a coarse filter, leaving jagged
/// edges, so it gets a bitmap drawn at the small-icon size of the taskbar's
/// display scale (#728).
#[cfg(windows)]
const TRAY_ICONS: [(u32, &[u8]); 6] = [
    (16, include_bytes!("../icons/tray/16x16.png")),
    (20, include_bytes!("../icons/tray/20x20.png")),
    (24, include_bytes!("../icons/tray/24x24.png")),
    (32, include_bytes!("../icons/tray/32x32.png")),
    (40, include_bytes!("../icons/tray/40x40.png")),
    (48, include_bytes!("../icons/tray/48x48.png")),
];

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
    tauri::image::Image::from_bytes(tray_icon_bytes(icon_size()))
        .expect("embedded tray icon is valid PNG")
}

#[cfg(windows)]
fn tray_icon_bytes(size: u32) -> &'static [u8] {
    let (_, largest) = TRAY_ICONS[TRAY_ICONS.len() - 1];
    TRAY_ICONS
        .iter()
        .find(|(width, _)| *width >= size)
        .map_or(largest, |(_, bytes)| bytes)
}

#[cfg(not(windows))]
fn tray_icon_bytes(_size: u32) -> &'static [u8] {
    TRAY_ICON_BYTES
}

/// Width of the icons the Windows taskbar shows, at its display's scale; it
/// changes with that scale. Other platforms size the icon themselves: 0.
#[cfg(windows)]
pub fn icon_size() -> u32 {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SM_CXSMICON};
    let class: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
    // SAFETY: the class name is NUL-terminated, and both calls accept a null
    // or foreign window handle.
    let dpi = unsafe {
        let taskbar = FindWindowW(class.as_ptr(), std::ptr::null());
        if taskbar.is_null() {
            0
        } else {
            GetDpiForWindow(taskbar)
        }
    };
    let dpi = if dpi == 0 { 96 } else { dpi };
    // SAFETY: plain metric query.
    u32::try_from(unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }).unwrap_or(16)
}

#[cfg(not(windows))]
pub fn icon_size() -> u32 {
    0
}

/// Dot color of the activity indicator.
const ACTIVITY_DOT: [u8; 3] = [0x22, 0xc5, 0x5e];

/// The tray bitmap, with a green dot in the lower right while downloads run.
pub fn tray_icon_image_with_activity(active: bool) -> tauri::image::Image<'static> {
    with_activity(tray_icon_image(), active)
}

fn with_activity(base: tauri::image::Image<'static>, active: bool) -> tauri::image::Image<'static> {
    if !active {
        return base;
    }
    let (width, height) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    paint_activity_dot(&mut rgba, width, height);
    tauri::image::Image::new_owned(rgba, width, height)
}

/// Clears a ring around the dot so it reads on any part of the logo, then
/// paints the dot with anti-aliased edges.
fn paint_activity_dot(rgba: &mut [u8], width: u32, height: u32) {
    let size = width.min(height) as f32;
    let radius = size * 0.2;
    let gap = size * 0.06;
    let cx = width as f32 - radius - 1.0;
    let cy = height as f32 - radius - 1.0;
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            let cut = (radius + gap - distance + 0.5).clamp(0.0, 1.0);
            if cut == 0.0 {
                continue;
            }
            let dot = (radius - distance + 0.5).clamp(0.0, 1.0);
            let pixel = ((y * width + x) * 4) as usize;
            let under = f32::from(rgba[pixel + 3]) / 255.0 * (1.0 - cut);
            let alpha = dot + under * (1.0 - dot);
            for (channel, &color) in ACTIVITY_DOT.iter().enumerate() {
                let value =
                    f32::from(color) * dot + f32::from(rgba[pixel + channel]) * under * (1.0 - dot);
                rgba[pixel + channel] = if alpha > 0.0 {
                    (value / alpha).round() as u8
                } else {
                    0
                };
            }
            rgba[pixel + 3] = (alpha * 255.0).round() as u8;
        }
    }
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
    items: Mutex<HashMap<String, MenuItem<tauri::Wry>>>,
    float_menu: Submenu<tauri::Wry>,
    float_modes: Vec<(FloatMode, CheckMenuItem<tauri::Wry>)>,
}

#[cfg(not(target_os = "linux"))]
impl TrayMenuState {
    pub fn set_labels(&self, labels: &serde_json::Value) {
        let Some(labels) = labels.as_object() else {
            return;
        };
        let items = self
            .items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (id, text) in labels {
            let Some(text) = text.as_str() else {
                continue;
            };
            let result = if let Some(item) = items.get(id.as_str()) {
                item.set_text(text)
            } else if self.float_menu.id() == id {
                self.float_menu.set_text(text)
            } else if let Some((_, item)) = self
                .float_modes
                .iter()
                .find(|(mode, _)| mode.menu_id() == id)
            {
                item.set_text(text)
            } else {
                Ok(())
            };
            if let Err(error) = result {
                log::debug!("tray:label-update-failed id={id} error={error}");
            }
        }
    }

    pub fn set_float_mode(&self, mode: FloatMode) {
        for (item_mode, item) in &self.float_modes {
            if let Err(error) = item.set_checked(*item_mode == mode) {
                log::debug!("tray:float-mode-update-failed error={error}");
            }
        }
    }
}

/// Checks the floating window's mode in the tray menu.
pub fn set_float_mode(app: &AppHandle, mode: FloatMode) {
    if let Some(state) = app.try_state::<TrayMenuState>() {
        state.set_float_mode(mode);
    }
}

/// Fallback labels until the frontend sends localized ones.
const FLOAT_MENU_LABEL: &str = "Floating Window";

fn float_mode_label(mode: FloatMode) -> &'static str {
    match mode {
        FloatMode::Always => "Always",
        FloatMode::Downloading => "While Downloading",
        FloatMode::Off => "Off",
    }
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
    crate::services::add_window::main_window_created(app);
    crate::services::float_window::main_window_created();

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

    crate::restore_deferred_window_state(window.app_handle(), window);
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

    let saved = crate::services::float_window::saved_mode(app);
    let float_modes = FloatMode::ALL
        .into_iter()
        .map(|mode| {
            let item = CheckMenuItem::with_id(
                app,
                mode.menu_id(),
                float_mode_label(mode),
                true,
                mode == saved,
                None::<&str>,
            )?;
            Ok((mode, item))
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let float_items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = float_modes
        .iter()
        .map(|(_, item)| item as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let float_menu =
        Submenu::with_id_and_items(app, "tray-float", FLOAT_MENU_LABEL, true, &float_items)?;

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
            &float_menu,
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
    if !crate::read_pref_bool(app, "showTrayIcon", true) {
        _tray.set_visible(false)?;
    }

    Ok(TrayMenuState {
        items: Mutex::new(items_map),
        float_menu,
        float_modes,
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
                    if let Err(e) =
                        crate::services::tasks::bittorrent::pause(&app, &aria2.0, None, true).await
                    {
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
                    match crate::services::tasks::bittorrent::resume_all(&app, &aria2.0).await {
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
        id => {
            let Some(mode) = FloatMode::from_menu_id(id) else {
                return;
            };
            if let Err(error) = crate::services::float_window::set_mode(app, mode) {
                log::warn!("tray:float-mode-failed error={error}");
            }
            // A click toggles the item it hits; the chosen mode stays checked.
            set_float_mode(app, crate::services::float_window::saved_mode(app));
        }
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

/// Shows or hides the tray icon; closing the window then minimizes instead of hiding.
pub fn set_visible(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(state) = app.try_state::<TrayMenuState>() {
        state.set_visible(visible);
    }
    #[cfg(not(target_os = "linux"))]
    if let Some(tray) = app.tray_by_id("rayburst") {
        tray.set_visible(visible)?;
    }
    Ok(())
}

/// Marks running downloads on the icon and puts the speeds in the tooltip.
///
/// macOS keeps its template icon, which cannot carry a colored dot; the
/// menu bar speedometer covers it there.
pub fn set_activity(app: &AppHandle, active: bool, tooltip: &str) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(state) = app.try_state::<TrayMenuState>() {
        state.set_activity(active, tooltip);
    }
    #[cfg(not(target_os = "linux"))]
    if let Some(tray) = app.tray_by_id("rayburst") {
        #[cfg(not(target_os = "macos"))]
        tray.set_icon(Some(tray_icon_image_with_activity(active)))?;
        #[cfg(target_os = "macos")]
        let _ = active;
        tray.set_tooltip(Some(tooltip))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_dot_paints_the_corner_and_keeps_the_rest() {
        let plain = tauri::image::Image::from_bytes(TRAY_ICON_BYTES).unwrap();
        let marked = with_activity(plain.clone(), true);
        let (width, height) = (plain.width() as usize, plain.height() as usize);
        assert_eq!(
            (marked.width() as usize, marked.height() as usize),
            (width, height)
        );
        let corner = ((height - 10) * width + (width - 10)) * 4;
        assert_eq!(
            &marked.rgba()[corner..corner + 4],
            &[0x22, 0xc5, 0x5e, 0xff]
        );
        assert_eq!(&marked.rgba()[..4], &plain.rgba()[..4]);
        assert_eq!(with_activity(plain.clone(), false).rgba(), plain.rgba());
    }

    #[cfg(windows)]
    #[test]
    fn each_display_scale_gets_a_bitmap_of_its_icon_size() {
        for (size, bytes) in TRAY_ICONS {
            let image = tauri::image::Image::from_bytes(bytes).unwrap();
            assert_eq!((image.width(), image.height()), (size, size));
            assert_eq!(tray_icon_bytes(size), bytes);
        }
        // Between sizes the next larger one; past the largest, the largest.
        assert_eq!(tray_icon_bytes(18), TRAY_ICONS[1].1);
        assert_eq!(tray_icon_bytes(64), TRAY_ICONS[5].1);
    }
}
