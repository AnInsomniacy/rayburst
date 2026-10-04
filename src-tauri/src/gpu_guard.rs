//! Linux WebKitGTK GPU rendering guard.
//!
//! WebKitGTK hardware rendering can crash on some GPU, driver, and Wayland
//! compositor combinations. Retain WebKitGTK defaults, with a scoped NVIDIA
//! Wayland explicit-sync workaround and an opt-in software fallback. External environment overrides remain owned
//! by the launching environment.

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

#[cfg(target_os = "linux")]
fn disable_webkit_hardware_rendering_with_marker() {
    unsafe {
        std::env::set_var(WEBKIT_DISABLE_DMABUF_RENDERER, "1");
        std::env::set_var(WEBKIT_DISABLE_COMPOSITING_MODE, "1");
        std::env::set_var(SELF_SET_MARKER, "1");
    }
}

#[cfg(target_os = "linux")]
pub fn pre_flight() {
    // NVIDIA's EGL Wayland path can enable explicit sync before GTK supplies
    // an acquire point. Keep acceleration and disable only that driver feature.
    let wayland = std::env::var("GDK_BACKEND").map_or_else(
        |_| std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland"),
        |backend| backend == "wayland",
    );
    if wayland
        && std::path::Path::new("/sys/module/nvidia").exists()
        && std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none()
    {
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
    } else {
        guard_log("gpu_guard: using WebKitGTK rendering defaults");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn pre_flight() {}

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
    let disable_override_source = if std::env::var_os(SELF_SET_MARKER).is_some() {
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
}
