//! What the platform draws around and behind every window.
//!
//! Windows are opaque: the system draws their corners, shadow and border
//! (Windows 11, macOS) and animates them, content and all, as they appear and
//! go, as the desktop environment does on Linux. Until a page paints, a window
//! and its webview show the background of the current theme, which the pages
//! report; windows built later start from it.

use std::sync::Mutex;

use tauri::utils::config::{Color as ConfigColor, WindowConfig};
use tauri::window::Color;
use tauri::WebviewWindow;

/// The theme's window background, as the last page reported it.
static BACKGROUND: Mutex<Option<(u8, u8, u8)>> = Mutex::new(None);

/// Paints `window` and its webview in the theme's background and remembers
/// the color for windows built later.
pub fn set_background(window: &WebviewWindow, rgb: (u8, u8, u8)) -> tauri::Result<()> {
    *BACKGROUND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(rgb);
    window.set_background_color(Some(Color(rgb.0, rgb.1, rgb.2, 255)))
}

/// Starts a window in the theme's background, once a page has reported it.
pub fn with_background(config: &mut WindowConfig) {
    let rgb = *BACKGROUND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((red, green, blue)) = rgb {
        config.background_color = Some(ConfigColor(red, green, blue, 255));
    }
}

/// macOS rounds and shadows only titled windows; a floating window keeps its
/// (transparent, full-size) title bar for that and hides its traffic lights.
pub fn hide_window_buttons(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    on_ns_window(window, |ns_window| {
        use objc2_app_kit::NSWindowButton;
        for kind in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = ns_window.standardWindowButton(kind) {
                button.setHidden(true);
            }
        }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

/// Runs `apply` with the window's `NSWindow` on the main thread, which owns it.
#[cfg(target_os = "macos")]
fn on_ns_window(window: &WebviewWindow, apply: fn(&objc2_app_kit::NSWindow)) {
    let target = window.clone();
    let scheduled = window.run_on_main_thread(move || {
        let Ok(pointer) = target.ns_window() else {
            return;
        };
        // SAFETY: Tauri hands out the window's live NSWindow, and this runs on
        // the main thread that owns it.
        let ns_window = unsafe { &*pointer.cast::<objc2_app_kit::NSWindow>() };
        apply(ns_window);
    });
    if let Err(error) = scheduled {
        log::debug!("native_window:main-thread-unavailable error={error}");
    }
}
