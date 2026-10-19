//! Linux WebKitGTK GPU rendering guard.
//!
//! WebKitGTK hardware rendering can crash on some GPU, driver, and Wayland
//! compositor combinations. Retain WebKitGTK defaults, with scoped NVIDIA
//! workarounds and a Mesa software compositor for software mode and NVIDIA.
//! External environment overrides
//! remain owned by the launching environment.
//!
//! Every webview repaints whole frames: WebKitGTK 2.54 composites only the
//! damaged regions, and on surfaces that keep no previous frame (without
//! DMA-BUF) the rest of the window stays blank (#650, #705).
//!
//! Wayland lets no client place its windows or keep them on top (tao#1134),
//! which the floating orb needs; with the orb on, the app runs under XWayland.

#[cfg(target_os = "linux")]
use std::{collections::BTreeMap, io::Write, path::Path};

#[cfg(target_os = "linux")]
const SELF_SET_MARKER: &str = "_DESKTOP_WEBKIT_RENDERING_SELF_SET";

#[cfg(target_os = "linux")]
const WEBKIT_DISABLE_DMABUF_RENDERER: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

#[cfg(target_os = "linux")]
const WEBKIT_DISABLE_COMPOSITING_MODE: &str = "WEBKIT_DISABLE_COMPOSITING_MODE";

#[cfg(target_os = "linux")]
const WEBKIT_DMABUF_RENDERER_FORCE_SHM: &str = "WEBKIT_DMABUF_RENDERER_FORCE_SHM";

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

/// Whether the floating orb shows at all, the `floatWindow` preference.
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

/// Only these variables may be removed when a child process inherits our marker.
#[cfg(target_os = "linux")]
const MANAGED_RENDERING_VARIABLES: &[&str] = &[
    WEBKIT_DISABLE_DMABUF_RENDERER,
    WEBKIT_DISABLE_COMPOSITING_MODE,
    WEBKIT_DMABUF_RENDERER_FORCE_SHM,
    "WEBKIT_SKIA_ENABLE_CPU_RENDERING",
    "LIBGL_ALWAYS_SOFTWARE",
    "__GLX_VENDOR_LIBRARY_NAME", // Release the GLX override assigned by earlier builds.
    "__EGL_VENDOR_LIBRARY_FILENAMES",
];

#[cfg(target_os = "linux")]
const NVIDIA_DRIVER: &str = "nvidia";

#[cfg(target_os = "linux")]
#[derive(serde::Serialize, serde::Deserialize)]
struct RenderingOverrides {
    source: String,
    variables: BTreeMap<String, String>,
}

#[cfg(target_os = "linux")]
fn inherited_overrides(marker: &str) -> Option<RenderingOverrides> {
    // Older builds recorded only the reason and always assigned these values.
    match marker {
        "software" | NVIDIA_DRIVER => Some(RenderingOverrides {
            source: marker.into(),
            variables: if marker == "software" {
                [
                    (WEBKIT_DISABLE_DMABUF_RENDERER, "1"),
                    (WEBKIT_DISABLE_COMPOSITING_MODE, "1"),
                ]
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect()
            } else {
                [(WEBKIT_DISABLE_DMABUF_RENDERER.into(), "1".into())].into()
            },
        }),
        _ => serde_json::from_str(marker).ok(),
    }
}

#[cfg(target_os = "linux")]
fn owns_override(key: &str, value: &str, current: Option<&std::ffi::OsStr>) -> bool {
    MANAGED_RENDERING_VARIABLES.contains(&key) && current == Some(std::ffi::OsStr::new(value))
}

#[cfg(target_os = "linux")]
fn nvidia_driver_loaded() -> bool {
    std::path::Path::new("/sys/module/nvidia").exists()
}

#[cfg(target_os = "linux")]
fn mesa_egl_vendor(directories: &[&Path]) -> Option<std::path::PathBuf> {
    directories.iter().find_map(|directory| {
        let mut paths: Vec<_> = std::fs::read_dir(directory)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        paths.sort();
        paths.into_iter().find(|path| {
            if path.extension().is_none_or(|extension| extension != "json") {
                return false;
            }
            let library = (|| -> Option<String> {
                let json: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
                Some(json.get("ICD")?.get("library_path")?.as_str()?.into())
            })();
            library.is_some_and(|library| library.contains("libEGL_mesa"))
        })
    })
}

#[cfg(target_os = "linux")]
fn rendering_environment(
    software: bool,
    nvidia: bool,
    mesa_vendor: Option<&Path>,
) -> BTreeMap<String, String> {
    let mut variables = BTreeMap::new();
    if !software && !nvidia {
        return variables;
    }
    if nvidia && mesa_vendor.is_none() {
        // Retain the existing crash fallback when no Mesa EGL implementation exists.
        variables.insert(WEBKIT_DISABLE_DMABUF_RENDERER.into(), "1".into());
        if software {
            variables.insert(WEBKIT_DISABLE_COMPOSITING_MODE.into(), "1".into());
        }
        return variables;
    }
    // Skia's non-composited backing store in WebKitGTK 2.50.4 blends updates
    // with SrcOver even when Copy was requested (ShareableBitmapSkia::paint).
    // Keep the compositor using SHM. DISABLE_DMABUF_RENDERER also disables
    // AcceleratedBackingStore::checkRequirements, even under the Always policy.
    // CPU painting and Mesa EGL bypass the NVIDIA buffer import path.
    for (key, value) in [
        (WEBKIT_DMABUF_RENDERER_FORCE_SHM, "1"),
        ("WEBKIT_SKIA_ENABLE_CPU_RENDERING", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
    ] {
        variables.insert(key.into(), value.into());
    }
    // Leave GLX with the display server's vendor. Forcing Mesa GLX on an
    // NVIDIA X server can make GTK's RGBA GL-context probe fail and silently
    // send WebKit back through the broken non-composited backing store.
    if let Some(path) = mesa_vendor {
        variables.insert(
            "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
            path.to_string_lossy().into_owned(),
        );
    }
    variables
}

#[cfg(target_os = "linux")]
fn install_overrides(source: &str, variables: BTreeMap<String, String>) {
    let overrides = RenderingOverrides {
        source: source.into(),
        variables: variables
            .into_iter()
            .filter(|(key, _)| std::env::var_os(key).is_none())
            .collect(),
    };
    let marker = match serde_json::to_string(&overrides) {
        Ok(marker) => marker,
        Err(error) => {
            guard_log(&format!(
                "gpu_guard: rendering overrides not applied: {error}"
            ));
            return;
        }
    };
    // SAFETY: Called before Tauri and its worker threads are initialized.
    unsafe {
        for (key, value) in &overrides.variables {
            std::env::set_var(key, value);
        }
        std::env::set_var(SELF_SET_MARKER, marker);
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
    if let Ok(marker) = std::env::var(SELF_SET_MARKER) {
        if let Some(overrides) = inherited_overrides(&marker) {
            // SAFETY: Called before Tauri and its worker threads are initialized.
            unsafe {
                for (key, value) in overrides.variables {
                    if owns_override(&key, &value, std::env::var_os(&key).as_deref()) {
                        std::env::remove_var(key);
                    }
                }
                std::env::remove_var(SELF_SET_MARKER);
            }
            guard_log("gpu_guard: cleared inherited env vars from relaunch");
        }
    }
    if std::env::var_os(WEBKIT_DISABLE_DMABUF_RENDERER).is_some()
        || std::env::var_os(WEBKIT_DISABLE_COMPOSITING_MODE).is_some()
    {
        guard_log(&format!(
            "gpu_guard: external rendering overrides {WEBKIT_DISABLE_DMABUF_RENDERER}={:?} {WEBKIT_DISABLE_COMPOSITING_MODE}={:?}",
            std::env::var_os(WEBKIT_DISABLE_DMABUF_RENDERER),
            std::env::var_os(WEBKIT_DISABLE_COMPOSITING_MODE),
        ));
        return;
    }

    let software = data_dir().is_some_and(|dir| read_software_rendering_from_config(&dir));
    if software || nvidia {
        let mesa = mesa_egl_vendor(&[
            Path::new("/etc/glvnd/egl_vendor.d"),
            Path::new("/usr/share/glvnd/egl_vendor.d"),
        ]);
        let variables = rendering_environment(software, nvidia, mesa.as_deref());
        if variables.contains_key(WEBKIT_DISABLE_DMABUF_RENDERER) {
            guard_log("gpu_guard: Mesa EGL unavailable; legacy NVIDIA fallback may retain transparent frames");
        } else {
            guard_log("gpu_guard: CPU painting and Mesa SHM compositing; retaining the display's GLX vendor");
        }
        install_overrides(if software { "software" } else { NVIDIA_DRIVER }, variables);
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
    let shm_compositing = std::env::var(WEBKIT_DMABUF_RENDERER_FORCE_SHM)
        .is_ok_and(|value| value != "0")
        && !std::env::var(WEBKIT_DISABLE_DMABUF_RENDERER).is_ok_and(|value| value != "0")
        && !std::env::var(WEBKIT_DISABLE_COMPOSITING_MODE).is_ok_and(|value| value != "0");
    let configured = webview.with_webview(move |platform| {
        use webkit2gtk::{
            glib::object::ObjectType, HardwareAccelerationPolicy, SettingsExt, WebViewExt,
        };
        let Some(settings) = platform.inner().settings() else {
            return;
        };
        if shm_compositing {
            // Always retain the compositor, including after a CSS animation ends.
            // SHM carries complete frames without importing NVIDIA DMA-BUFs.
            settings.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
            log::info!("gpu_guard: requesting persistent SHM compositing");
        }
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
    let overrides = std::env::var(SELF_SET_MARKER)
        .ok()
        .and_then(|marker| inherited_overrides(&marker));
    let disable_override_source = if overrides
        .as_ref()
        .is_some_and(|overrides| overrides.source == NVIDIA_DRIVER)
    {
        "nvidia_driver"
    } else if overrides.is_some() {
        "software_rendering_preference"
    } else if dmabuf.is_some()
        || compositing.is_some()
        || environment(WEBKIT_DMABUF_RENDERER_FORCE_SHM).is_some()
    {
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
            "WEBKIT_DMABUF_RENDERER_FORCE_SHM": environment(WEBKIT_DMABUF_RENDERER_FORCE_SHM),
            "WEBKIT_DISABLE_COMPOSITING_MODE": compositing,
            "WEBKIT_USE_SKIA_FOR_COMPOSITION": environment("WEBKIT_USE_SKIA_FOR_COMPOSITION"),
            "WEBKIT_SKIA_ENABLE_CPU_RENDERING": environment("WEBKIT_SKIA_ENABLE_CPU_RENDERING"),
            "LIBGL_ALWAYS_SOFTWARE": environment("LIBGL_ALWAYS_SOFTWARE"),
            "__GLX_VENDOR_LIBRARY_NAME": environment("__GLX_VENDOR_LIBRARY_NAME"),
            "__EGL_VENDOR_LIBRARY_FILENAMES": environment("__EGL_VENDOR_LIBRARY_FILENAMES"),
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

    #[cfg(target_os = "linux")]
    #[test]
    fn software_rendering_keeps_compositing_on_the_cpu_with_nvidia() {
        let vendor = Path::new("/usr/share/glvnd/egl_vendor.d/50_mesa.json");
        let variables = rendering_environment(true, true, Some(vendor));
        assert!(!variables.contains_key(WEBKIT_DISABLE_COMPOSITING_MODE));
        assert!(!variables.contains_key(WEBKIT_DISABLE_DMABUF_RENDERER));
        assert!(!variables.contains_key("__GLX_VENDOR_LIBRARY_NAME"));
        for key in [
            WEBKIT_DMABUF_RENDERER_FORCE_SHM,
            "WEBKIT_SKIA_ENABLE_CPU_RENDERING",
            "LIBGL_ALWAYS_SOFTWARE",
        ] {
            assert_eq!(variables.get(key).map(String::as_str), Some("1"));
        }
        assert_eq!(
            variables
                .get("__EGL_VENDOR_LIBRARY_FILENAMES")
                .map(String::as_str),
            vendor.to_str()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn nvidia_uses_the_same_transparent_frame_workaround_without_the_software_preference() {
        let vendor = Path::new("/mesa.json");
        assert_eq!(
            rendering_environment(false, true, Some(vendor)),
            rendering_environment(true, true, Some(vendor)),
        );
        assert!(rendering_environment(false, false, Some(vendor)).is_empty());
        let legacy = rendering_environment(false, true, None);
        assert_eq!(legacy.len(), 1);
        assert_eq!(
            legacy
                .get(WEBKIT_DISABLE_DMABUF_RENDERER)
                .map(String::as_str),
            Some("1"),
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn only_nvidia_without_mesa_needs_the_legacy_fallback() {
        let nvidia = rendering_environment(true, true, None);
        assert_eq!(
            nvidia
                .get(WEBKIT_DISABLE_COMPOSITING_MODE)
                .map(String::as_str),
            Some("1")
        );
        let mesa = rendering_environment(true, false, None);
        assert!(!mesa.contains_key(WEBKIT_DISABLE_COMPOSITING_MODE));
        assert_eq!(
            mesa.get("LIBGL_ALWAYS_SOFTWARE").map(String::as_str),
            Some("1")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finds_mesa_by_its_library_rather_than_vendor_filename() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("10_nvidia.json"),
            r#"{"ICD":{"library_path":"libEGL_nvidia.so.0"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("20_invalid.json"), "invalid").unwrap();
        std::fs::write(
            dir.path().join("30_mesa.txt"),
            r#"{"ICD":{"library_path":"libEGL_mesa.so.0"}}"#,
        )
        .unwrap();
        assert_eq!(mesa_egl_vendor(&[dir.path()]), None);
        let mesa = dir.path().join("90_custom.json");
        std::fs::write(
            &mesa,
            r#"{"ICD":{"library_path":"/usr/lib/libEGL_mesa.so.0"}}"#,
        )
        .unwrap();
        assert_eq!(
            mesa_egl_vendor(&[&dir.path().join("absent"), dir.path()]),
            Some(mesa)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn relaunch_releases_only_recorded_and_unchanged_rendering_overrides() {
        let variables = rendering_environment(true, true, Some(Path::new("/mesa.json")));
        let marker = serde_json::to_string(&RenderingOverrides {
            source: "software".into(),
            variables: variables.clone(),
        })
        .unwrap();
        let inherited = inherited_overrides(&marker).unwrap();
        assert_eq!(inherited.variables, variables);
        for (key, value) in &inherited.variables {
            assert!(owns_override(key, value, Some(std::ffi::OsStr::new(value))));
            assert!(!owns_override(
                key,
                value,
                Some(std::ffi::OsStr::new("external"))
            ));
            assert!(!owns_override(key, value, None));
        }
        assert!(!owns_override(
            "PATH",
            "external",
            Some(std::ffi::OsStr::new("external"))
        ));
        assert!(inherited_overrides("invalid").is_none());
        let previous = inherited_overrides(
            r#"{"source":"software","variables":{"WEBKIT_DISABLE_DMABUF_RENDERER":"1","__GLX_VENDOR_LIBRARY_NAME":"mesa"}}"#,
        )
        .unwrap();
        for (key, value) in previous.variables {
            assert!(owns_override(
                &key,
                &value,
                Some(std::ffi::OsStr::new(&value))
            ));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn understands_inherited_markers_from_the_previous_rendering_strategy() {
        let software = inherited_overrides("software").unwrap();
        assert_eq!(software.variables.len(), 2);
        assert!(software
            .variables
            .contains_key(WEBKIT_DISABLE_COMPOSITING_MODE));
        let nvidia = inherited_overrides(NVIDIA_DRIVER).unwrap();
        assert_eq!(nvidia.source, NVIDIA_DRIVER);
        assert_eq!(nvidia.variables.len(), 1);
        assert!(!nvidia
            .variables
            .contains_key(WEBKIT_DISABLE_COMPOSITING_MODE));
    }

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
