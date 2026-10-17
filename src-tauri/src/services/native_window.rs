//! What the platform draws around and behind every window.
//!
//! Windows are opaque: the system draws their corners, shadow and border
//! (Windows 11, macOS) and animates them, content and all, as they appear and
//! go. Until a page paints, a window and its webview show the background of
//! the current theme, which the pages report; windows built later start from it.
//!
//! Each platform animates in its own way, and each is given what it needs to
//! animate both ways:
//!
//! - Windows animates windows as they show and as they are destroyed.
//! - macOS infers an animation from a window's kind and finds none for a
//!   floating window, so each window is told its kind. It fades a window out
//!   only while ordering it out with its content, and destroying the window
//!   removes the webview first; a closing window is ordered out and destroyed
//!   once its fade is over.
//! - On Linux the compositor animates windows on GNOME, KDE and their kin.
//!   Elsewhere a window fades its own opacity in and out, which X11
//!   compositors honor; without one it simply appears and goes.

use std::sync::Mutex;

use tauri::utils::config::{Color as ConfigColor, WindowConfig};
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewWindow};

/// How a window comes and goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Pops in and fades out, as a dialog does: the add window.
    Dialog,
    /// Fades in and out, as a palette does: the floating bar and its panel.
    Palette,
}

/// How long macOS takes to fade a window out before it can be destroyed.
#[cfg(target_os = "macos")]
const ORDER_OUT: std::time::Duration = std::time::Duration::from_millis(260);

/// Windows on their way out, so a second close does not repeat it.
static RETIRING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Tells the system how to animate a window built hidden, before it first shows.
pub fn prepare(window: &WebviewWindow, motion: Motion) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSWindow, NSWindowAnimationBehavior};
        let apply: fn(&NSWindow) = match motion {
            Motion::Dialog => {
                |window| window.setAnimationBehavior(NSWindowAnimationBehavior::AlertPanel)
            }
            Motion::Palette => {
                |window| window.setAnimationBehavior(NSWindowAnimationBehavior::UtilityWindow)
            }
        };
        on_ns_window(window, apply);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, motion);
}

/// Shows a window and gives it the focus.
pub fn present(window: &WebviewWindow) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    if fade::wanted() && !window.is_visible().unwrap_or(true) {
        let target = window.clone();
        return window.run_on_main_thread(move || {
            fade::set(&target, 0.0);
            let _ = target.show();
            let _ = target.set_focus();
            fade::run(target, fade::IN, |_| {});
        });
    }
    window.show()?;
    let _ = window.unminimize();
    window.set_focus()
}

/// Shows or hides a window without activating it. Asynchronous, so the pump
/// never waits on the thread that owns the window.
pub fn show_inactive(window: &WebviewWindow, visible: bool) {
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
    // Making the window key would take the focus from the window in use.
    #[cfg(target_os = "macos")]
    {
        let apply: fn(&objc2_app_kit::NSWindow) = if visible {
            |window| window.orderFrontRegardless()
        } else {
            |window| window.orderOut(None)
        };
        on_ns_window(window, apply);
    }
    #[cfg(target_os = "linux")]
    {
        if visible && fade::wanted() && !window.is_visible().unwrap_or(true) {
            let target = window.clone();
            let _ = window.run_on_main_thread(move || {
                fade::set(&target, 0.0);
                let _ = target.show();
                fade::run(target, fade::IN, |_| {});
            });
            return;
        }
        let _ = if visible {
            window.show()
        } else {
            window.hide()
        };
    }
}

/// Closes a window with the system's animation and destroys it.
pub fn retire(window: WebviewWindow) {
    let label = window.label().to_owned();
    {
        let mut retiring = RETIRING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if retiring.contains(&label) {
            return;
        }
        retiring.push(label.clone());
    }
    let destroy = move |window: WebviewWindow| {
        if let Err(error) = window.destroy() {
            log::warn!("native_window:destroy-failed label={label} error={error}");
        }
        RETIRING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|retired| *retired != label);
    };
    #[cfg(target_os = "macos")]
    {
        let _ = window.hide();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(ORDER_OUT).await;
            destroy(window);
        });
    }
    #[cfg(target_os = "linux")]
    if fade::wanted() && window.is_visible().unwrap_or(false) {
        let target = window.clone();
        if let Err(error) = window.run_on_main_thread(move || fade::run(target, fade::OUT, destroy))
        {
            log::debug!("native_window:main-thread-unavailable error={error}");
        }
    } else {
        destroy(window);
    }
    #[cfg(windows)]
    destroy(window);
}

/// Waits, off the main thread, until a window with `label` that is on its way
/// out is gone, so a new one can take its label.
pub fn wait_gone(app: &AppHandle, label: &str) {
    let start = std::time::Instant::now();
    while app.get_webview_window(label).is_some()
        && start.elapsed() < std::time::Duration::from_secs(1)
    {
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

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

/// The fade of a window on Linux desktops whose compositor does not animate.
#[cfg(target_os = "linux")]
mod fade {
    use std::time::{Duration, Instant};

    use gtk::prelude::WidgetExt;
    use tauri::WebviewWindow;

    /// From, to, and how long it takes: a fade in eases out, a fade out eases in.
    pub type Fade = (f64, f64, Duration);
    pub const IN: Fade = (0.0, 1.0, Duration::from_millis(180));
    pub const OUT: Fade = (1.0, 0.0, Duration::from_millis(140));

    /// Desktops whose compositor animates windows by itself.
    const ANIMATED: [&str; 9] = [
        "gnome",
        "kde",
        "cinnamon",
        "x-cinnamon",
        "budgie",
        "pantheon",
        "deepin",
        "cosmic",
        "unity",
    ];

    pub fn wanted() -> bool {
        static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *WANTED.get_or_init(|| !animates(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()))
    }

    /// Whether `XDG_CURRENT_DESKTOP`, a list separated by colons, names a
    /// desktop that animates its windows.
    pub fn animates(desktops: &str) -> bool {
        desktops
            .split(':')
            .any(|desktop| ANIMATED.contains(&desktop.trim().to_ascii_lowercase().as_str()))
    }

    /// Main thread only.
    pub fn set(window: &WebviewWindow, opacity: f64) {
        if let Ok(gtk_window) = window.gtk_window() {
            gtk_window.set_opacity(opacity);
        }
    }

    /// Fades the window, then hands it to `done`. Main thread only.
    pub fn run(
        window: WebviewWindow,
        (from, to, length): Fade,
        done: impl FnOnce(WebviewWindow) + 'static,
    ) {
        let start = Instant::now();
        let mut done = Some(done);
        gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
            let t = (start.elapsed().as_secs_f64() / length.as_secs_f64()).min(1.0);
            let eased = if to > from {
                1.0 - (1.0 - t).powi(3)
            } else {
                t * t * t
            };
            set(&window, from + (to - from) * eased);
            if t < 1.0 {
                return gtk::glib::ControlFlow::Continue;
            }
            if let Some(done) = done.take() {
                done(window.clone());
            }
            gtk::glib::ControlFlow::Break
        });
    }
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn desktops_that_animate_their_windows_keep_their_own_animation() {
        assert!(super::fade::animates("GNOME"));
        assert!(super::fade::animates("ubuntu:GNOME"));
        assert!(super::fade::animates("KDE"));
        assert!(!super::fade::animates("XFCE"));
        assert!(!super::fade::animates(""));
    }
}
