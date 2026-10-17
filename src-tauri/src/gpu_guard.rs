//! Linux WebKitGTK GPU rendering guard.
//!
//! WebKitGTK hardware rendering can crash on some GPU, driver, and Wayland
//! compositor combinations. Retain WebKitGTK defaults, with scoped NVIDIA
//! workarounds and an opt-in software fallback. External environment overrides
//! remain owned by the launching environment.
//!
//! Every webview repaints whole frames: WebKitGTK 2.54 composites only the
//! damaged regions, and on surfaces that keep no previous frame (without
//! DMA-BUF) the rest of the window stays blank (#650, #705).
//!
//! Wayland lets no client place its windows or keep them on top (tao#1134),
//! which the floating bar needs; with the bar on, the app runs under XWayland.

#[cfg(target_os = "linux")]
use std::io::Write;

#[cfg(target_os = "linux")]
const SELF_SET_MARKER: &str = "_DESKTOP_WEBKIT_RENDERING_SELF_SET";

#[cfg(target_os = "linux")]
const WEBKIT_DISABLE_DMABUF_RENDERER: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

#[cfg(target_os = "linux")]
const WEBKIT_DISABLE_COMPOSITING_MODE: &str = "WEBKIT_DISABLE_COMPOSITING_MODE";

#[cfg(target_os = "linux")]
fn data_dir() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::APP_ID))
}

#[cfg(target_os = "linux")]
fn guard_log(message: &str) {
    eprintln!("[rayburst] {message}");
    if let Some(dir) = data_dir() {
        let log_dir = dir.join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("rayburst.log");
        let timestamp = chrono::Local::now().format("%Y-%m-%d][%H:%M:%S");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let _ = writeln!(file, "[{timestamp}][INFO][gpu_guard] {message}");
        }
    }
}

/// Whether the floating bar shows at all, the `floatWindow` preference.
#[cfg(any(target_os = "linux", test))]
fn read_float_window_from_config(data_dir: &std::path::Path) -> bool {
    (|| -> Option<bool> {
        let content = std::fs::read_to_string(data_dir.join("config.json")).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        let mode = json.get("preferences")?.get("floatWindow")?.as_str()?;
        Some(matches!(mode, "always" | "downloading"))
    })()
    .unwrap_or(false)
}

#[cfg(any(target_os = "linux", test))]
fn read_software_rendering_from_config(data_dir: &std::path::Path) -> bool {
    (|| -> Option<bool> {
        let path = data_dir.join("config.json");
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        json.get("preferences")?.get("softwareRendering")?.as_bool()
    })()
    .unwrap_or(false)
}

/// Marker values naming why this process disabled WebKitGTK rendering paths.
#[cfg(target_os = "linux")]
const SOFTWARE_RENDERING: &str = "software";
#[cfg(target_os = "linux")]
const NVIDIA_DRIVER: &str = "nvidia";

#[cfg(target_os = "linux")]
fn nvidia_driver_loaded() -> bool {
    std::path::Path::new("/sys/module/nvidia").exists()
}

#[cfg(target_os = "linux")]
fn disable_webkit_hardware_rendering_with_marker() {
    unsafe {
        std::env::set_var(WEBKIT_DISABLE_DMABUF_RENDERER, "1");
        std::env::set_var(WEBKIT_DISABLE_COMPOSITING_MODE, "1");
        std::env::set_var(SELF_SET_MARKER, SOFTWARE_RENDERING);
    }
}

/// NVIDIA's GBM cannot allocate the buffers the DMA-BUF renderer asks for, and
/// the window then stays blank (tauri#13493).
#[cfg(target_os = "linux")]
fn disable_dmabuf_renderer_with_marker() {
    unsafe {
        std::env::set_var(WEBKIT_DISABLE_DMABUF_RENDERER, "1");
        std::env::set_var(SELF_SET_MARKER, NVIDIA_DRIVER);
    }
}

#[cfg(target_os = "linux")]
pub fn pre_flight() {
    // A backend chosen by the launching environment is kept.
    // GDK picks Wayland when a Wayland display is set; some sessions (WSLg)
    // set no XDG_SESSION_TYPE.
    let wayland_display =
        std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
    if std::env::var_os("GDK_BACKEND").is_none()
        && (wayland_display || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland"))
        && std::env::var_os("DISPLAY").is_some()
        && data_dir().is_some_and(|dir| read_float_window_from_config(&dir))
    {
        // SAFETY: Called before Tauri and its worker threads are initialized.
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
        }
        guard_log("gpu_guard: floating window on Wayland, running under XWayland (tao#1134)");
    }
    // NVIDIA's EGL Wayland path can enable explicit sync before GTK supplies
    // an acquire point. Disable only that driver feature.
    let wayland = std::env::var("GDK_BACKEND").map_or_else(
        |_| std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland"),
        |backend| backend == "wayland",
    );
    let nvidia = nvidia_driver_loaded();
    if wayland && nvidia && std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
        // SAFETY: Called before Tauri and its worker threads are initialized.
        unsafe {
            std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
        }
        guard_log("gpu_guard: disabled NVIDIA Wayland explicit sync (WebKit #324551)");
    }
    if std::env::var_os(SELF_SET_MARKER).is_some() {
        unsafe {
            std::env::remove_var(SELF_SET_MARKER);
            std::env::remove_var(WEBKIT_DISABLE_DMABUF_RENDERER);
            std::env::remove_var(WEBKIT_DISABLE_COMPOSITING_MODE);
        }
        guard_log("gpu_guard: cleared inherited env vars from relaunch");
    } else if std::env::var_os(WEBKIT_DISABLE_DMABUF_RENDERER).is_some()
        || std::env::var_os(WEBKIT_DISABLE_COMPOSITING_MODE).is_some()
    {
        guard_log(&format!(
            "gpu_guard: external rendering overrides {WEBKIT_DISABLE_DMABUF_RENDERER}={:?} {WEBKIT_DISABLE_COMPOSITING_MODE}={:?}",
            std::env::var_os(WEBKIT_DISABLE_DMABUF_RENDERER),
            std::env::var_os(WEBKIT_DISABLE_COMPOSITING_MODE),
        ));
        return;
    }

    if data_dir().is_some_and(|dir| read_software_rendering_from_config(&dir)) {
        disable_webkit_hardware_rendering_with_marker();
        guard_log("gpu_guard: explicit software rendering fallback");
    } else if nvidia {
        disable_dmabuf_renderer_with_marker();
        guard_log("gpu_guard: disabled the DMA-BUF renderer for the NVIDIA driver (tauri#13493)");
    } else {
        guard_log("gpu_guard: using WebKitGTK rendering defaults");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn pre_flight() {}

/// WebKit preference that limits compositing to the damaged rectangles of a
/// frame, on by default since WebKitGTK 2.54. Where the window surface does
/// not keep the previous frame (transparent windows, SHM and NVIDIA buffers)
/// everything outside them blinks out (#650, #705; Handy#2166).
#[cfg(target_os = "linux")]
const DAMAGE_COMPOSITING_FEATURE: &str = "UseDamagingInformationForCompositing";

/// Every webview repaints whole frames: Rayburst's pages are small and mostly
/// still, so the saving the damage tracking buys is negligible next to frames
/// that lose their content. A WebKitGTK without the preference is left as is.
#[cfg(target_os = "linux")]
pub fn configure_webview<R: tauri::Runtime>(webview: &tauri::Webview<R>) {
    let configured = webview.with_webview(|platform| {
        use webkit2gtk::{glib::object::ObjectType, WebViewExt};
        let Some(settings) = platform.inner().settings() else {
            return;
        };
        // SAFETY: the settings object is alive for the duration of the call,
        // which runs on the GTK thread that owns it.
        let disabled = unsafe {
            webkit_features::disable(settings.as_ptr().cast(), DAMAGE_COMPOSITING_FEATURE)
        };
        if disabled {
            log::info!("gpu_guard: whole-frame repaints (disabled {DAMAGE_COMPOSITING_FEATURE})");
        }
    });
    if let Err(error) = configured {
        log::warn!("gpu_guard: webview not configured error={error}");
    }
}

/// WebKitGTK's feature API (2.42+), looked up at run time: binding it at link
/// time would keep the app from starting on an older WebKitGTK.
#[cfg(target_os = "linux")]
mod webkit_features {
    use std::ffi::{c_char, c_int, c_void, CStr};

    type AllFeatures = unsafe extern "C" fn() -> *mut c_void;
    type ListLength = unsafe extern "C" fn(*mut c_void) -> usize;
    type ListGet = unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void;
    type ListUnref = unsafe extern "C" fn(*mut c_void);
    type Identifier = unsafe extern "C" fn(*mut c_void) -> *const c_char;
    type SetEnabled = unsafe extern "C" fn(*mut c_void, *mut c_void, c_int);

    unsafe fn symbol<T>(name: &CStr) -> Option<T> {
        let address = libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr());
        // SAFETY: the caller names a WebKitGTK function of signature `T`.
        (!address.is_null()).then(|| std::mem::transmute_copy(&address))
    }

    /// Turns `identifier` off in `settings`; false when this WebKitGTK lacks
    /// the API or the feature.
    ///
    /// # Safety
    /// `settings` must point to a live `WebKitSettings`.
    pub unsafe fn disable(settings: *mut c_void, identifier: &str) -> bool {
        let (Some(all), Some(length), Some(get), Some(unref), Some(id), Some(set)) = (
            symbol::<AllFeatures>(c"webkit_settings_get_all_features"),
            symbol::<ListLength>(c"webkit_feature_list_get_length"),
            symbol::<ListGet>(c"webkit_feature_list_get"),
            symbol::<ListUnref>(c"webkit_feature_list_unref"),
            symbol::<Identifier>(c"webkit_feature_get_identifier"),
            symbol::<SetEnabled>(c"webkit_settings_set_feature_enabled"),
        ) else {
            return false;
        };
        let list = all();
        if list.is_null() {
            return false;
        }
        let found = (0..length(list))
            .map(|index| get(list, index))
            .find(|feature| {
                let name = id(*feature);
                !name.is_null() && CStr::from_ptr(name).to_bytes() == identifier.as_bytes()
            });
        if let Some(feature) = found {
            set(settings, feature, 0);
        }
        unref(list);
        found.is_some()
    }
}

#[cfg(target_os = "linux")]
async fn hardware_acceleration_policy(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    use webkit2gtk::{SettingsExt, WebViewExt};

    // Inspect the existing WebView on its UI thread; never recreate a hidden one.
    let window = app
        .get_webview_window("main")
        .ok_or("The main WebView is not available")?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window
        .with_webview(move |webview| {
            let policy = webview
                .inner()
                .settings()
                .map(|settings| format!("{:?}", settings.hardware_acceleration_policy()))
                .ok_or("WebKitGTK settings are not available");
            let _ = sender.send(policy);
        })
        .map_err(|error| format!("Failed to inspect the WebView: {error}"))?;
    tokio::time::timeout(std::time::Duration::from_secs(2), receiver)
        .await
        .map_err(|_| "Timed out reading the WebKitGTK policy")?
        .map_err(|_| "The WebView closed before its policy could be read")?
        .map_err(str::to_string)
}

#[cfg(target_os = "linux")]
pub async fn diagnostic_snapshot(app: &tauri::AppHandle) -> serde_json::Value {
    let environment =
        |name| std::env::var_os(name).map(|value| value.to_string_lossy().into_owned());
    let dmabuf = environment(WEBKIT_DISABLE_DMABUF_RENDERER);
    let compositing = environment(WEBKIT_DISABLE_COMPOSITING_MODE);
    let marker = std::env::var(SELF_SET_MARKER).ok();
    let disable_override_source = if marker.as_deref() == Some(NVIDIA_DRIVER) {
        "nvidia_driver"
    } else if marker.is_some() {
        "software_rendering_preference"
    } else if dmabuf.is_some() || compositing.is_some() {
        "environment"
    } else {
        "none"
    };
    let version = tauri::webview_version();
    let policy = hardware_acceleration_policy(app).await;
    // A native policy describes allowed behavior, not proof of active GPU rendering.
    serde_json::json!({
        "webkitgtk_version": version.as_ref().ok(),
        "version_error": version.err().map(|error| error.to_string()),
        "hardware_acceleration_policy": policy.as_ref().ok(),
        "policy_error": policy.as_ref().err(),
        "disable_override_source": disable_override_source,
        "environment": {
            "WEBKIT_DISABLE_DMABUF_RENDERER": dmabuf,
            "WEBKIT_DISABLE_COMPOSITING_MODE": compositing,
            "WEBKIT_USE_SKIA_FOR_COMPOSITION": environment("WEBKIT_USE_SKIA_FOR_COMPOSITION"),
            "__NV_DISABLE_EXPLICIT_SYNC": environment("__NV_DISABLE_EXPLICIT_SYNC"),
            "GDK_BACKEND": environment("GDK_BACKEND"),
            "XDG_SESSION_TYPE": environment("XDG_SESSION_TYPE"),
        },
    })
}

#[cfg(not(target_os = "linux"))]
pub async fn diagnostic_snapshot(_app: &tauri::AppHandle) -> serde_json::Value {
    serde_json::Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn software_fallback_requires_an_explicit_current_setting() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!read_software_rendering_from_config(dir.path()));
        for (config, enabled) in [
            (r#"{"preferences":{"softwareRendering":true}}"#, true),
            (r#"{"preferences":{"softwareRendering":false}}"#, false),
            (r#"{"preferences":{}}"#, false),
            ("invalid", false),
        ] {
            std::fs::write(dir.path().join("config.json"), config).unwrap();
            assert_eq!(read_software_rendering_from_config(dir.path()), enabled);
        }
    }

    #[test]
    fn only_a_shown_floating_window_asks_for_xwayland() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!read_float_window_from_config(dir.path()));
        for (config, enabled) in [
            (r#"{"preferences":{"floatWindow":"always"}}"#, true),
            (r#"{"preferences":{"floatWindow":"downloading"}}"#, true),
            (r#"{"preferences":{"floatWindow":"off"}}"#, false),
            (r#"{"preferences":{}}"#, false),
        ] {
            std::fs::write(dir.path().join("config.json"), config).unwrap();
            assert_eq!(read_float_window_from_config(dir.path()), enabled);
        }
    }
}
