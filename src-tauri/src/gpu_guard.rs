//! Linux WebKitGTK GPU rendering guard.
//!
//! WebKitGTK hardware rendering can crash on some GPU, driver, and Wayland
//! compositor combinations. Retain WebKitGTK defaults, with scoped NVIDIA
//! workarounds; software mode and NVIDIA paint on the CPU and composite over
//! shared memory. External environment overrides remain owned by the
//! launching environment.
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
const WEBKIT_SKIA_ENABLE_CPU_RENDERING: &str = "WEBKIT_SKIA_ENABLE_CPU_RENDERING";

#[cfg(target_os = "linux")]
const LIBGL_ALWAYS_SOFTWARE: &str = "LIBGL_ALWAYS_SOFTWARE";

#[cfg(target_os = "linux")]
const EGL_VENDOR_LIBRARY_FILENAMES: &str = "__EGL_VENDOR_LIBRARY_FILENAMES";

#[cfg(target_os = "linux")]
const GDK_BACKEND: &str = "GDK_BACKEND";

#[cfg(target_os = "linux")]
const NV_DISABLE_EXPLICIT_SYNC: &str = "__NV_DISABLE_EXPLICIT_SYNC";

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

/// Variables this module may set. Only these are cleared when a relaunched
/// process inherits them; `__GLX_VENDOR_LIBRARY_NAME` was set by earlier builds.
#[cfg(target_os = "linux")]
const MANAGED_VARIABLES: &[&str] = &[
    WEBKIT_DISABLE_DMABUF_RENDERER,
    WEBKIT_DISABLE_COMPOSITING_MODE,
    WEBKIT_DMABUF_RENDERER_FORCE_SHM,
    WEBKIT_SKIA_ENABLE_CPU_RENDERING,
    LIBGL_ALWAYS_SOFTWARE,
    EGL_VENDOR_LIBRARY_FILENAMES,
    "__GLX_VENDOR_LIBRARY_NAME",
    GDK_BACKEND,
    NV_DISABLE_EXPLICIT_SYNC,
];

#[cfg(target_os = "linux")]
const NVIDIA_DRIVER: &str = "nvidia";

/// The variables a process set itself, named in its marker. A relaunch inherits
/// them; the new process clears them and decides again, while variables set by
/// the launching environment stay. Earlier builds named only a reason.
#[cfg(target_os = "linux")]
fn inherited_variables(marker: &str) -> Vec<&str> {
    match marker {
        "software" => vec![
            WEBKIT_DISABLE_DMABUF_RENDERER,
            WEBKIT_DISABLE_COMPOSITING_MODE,
        ],
        NVIDIA_DRIVER => vec![WEBKIT_DISABLE_DMABUF_RENDERER],
        _ => marker
            .split(',')
            .filter(|name| MANAGED_VARIABLES.contains(name))
            .collect(),
    }
}

/// Sets `variables` the launching environment left unset, and records them.
#[cfg(target_os = "linux")]
fn set_variables(variables: &BTreeMap<&'static str, String>) {
    for (key, value) in variables {
        if std::env::var_os(key).is_some() {
            continue;
        }
        // SAFETY: Called before Tauri and its worker threads are initialized.
        unsafe {
            std::env::set_var(key, value);
        }
        let marker = std::env::var(SELF_SET_MARKER).unwrap_or_default();
        let marker = if marker.is_empty() {
            (*key).to_owned()
        } else {
            format!("{marker},{key}")
        };
        // SAFETY: As above.
        unsafe {
            std::env::set_var(SELF_SET_MARKER, marker);
        }
    }
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
) -> BTreeMap<&'static str, String> {
    let mut variables = BTreeMap::new();
    if !software && !nvidia {
        return variables;
    }
    if nvidia && mesa_vendor.is_none() {
        // Without Mesa's EGL, keep NVIDIA's buffers out of the DMA-BUF
        // renderer, which otherwise leaves the window blank (tauri#13493).
        variables.insert(WEBKIT_DISABLE_DMABUF_RENDERER, "1".into());
        if software {
            variables.insert(WEBKIT_DISABLE_COMPOSITING_MODE, "1".into());
        }
        return variables;
    }
    // Without compositing, WebKitGTK 2.50's Skia backing store blends each
    // update over the last (ShareableBitmapSkia::paint): transparent windows
    // pile up shadows and old frames. The compositor is kept instead, handing
    // whole frames over shared memory; painting on the CPU with Mesa's EGL
    // keeps NVIDIA's buffers out of it.
    variables.insert(WEBKIT_DMABUF_RENDERER_FORCE_SHM, "1".into());
    variables.insert(WEBKIT_SKIA_ENABLE_CPU_RENDERING, "1".into());
    variables.insert(LIBGL_ALWAYS_SOFTWARE, "1".into());
    // GLX stays with the display server's vendor: Mesa's GLX on an NVIDIA X
    // server fails GTK's RGBA context and drops WebKit back to the blending path.
    if let Some(path) = mesa_vendor {
        variables.insert(
            EGL_VENDOR_LIBRARY_FILENAMES,
            path.to_string_lossy().into_owned(),
        );
    }
    variables
}

/// The display backend this process chose for the floating window, once.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum FloatBackend {
    /// No choice: an X11 session, no XWayland, or a backend the launching
    /// environment set.
    Given,
    /// A Wayland session without the orb at startup, running natively.
    Wayland,
    /// A Wayland session with the orb at startup, running under XWayland.
    XWayland,
}

#[cfg(target_os = "linux")]
static FLOAT_BACKEND: std::sync::OnceLock<FloatBackend> = std::sync::OnceLock::new();

/// Whether showing the floating window needs a restart: on Wayland it runs
/// under XWayland, which a process can only choose before GTK starts.
pub fn float_window_needs_relaunch(shown: bool) -> bool {
    #[cfg(target_os = "linux")]
    {
        shown && FLOAT_BACKEND.get() == Some(&FloatBackend::Wayland)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = shown;
        false
    }
}

#[cfg(target_os = "linux")]
pub fn pre_flight() {
    // A relaunch starts from the launching environment, not from this
    // process's own choices, which may no longer hold.
    if let Ok(marker) = std::env::var(SELF_SET_MARKER) {
        // SAFETY: Called before Tauri and its worker threads are initialized.
        unsafe {
            for key in inherited_variables(&marker) {
                std::env::remove_var(key);
            }
            std::env::remove_var(SELF_SET_MARKER);
        }
        guard_log("gpu_guard: cleared inherited env vars from relaunch");
    }
    // A backend chosen by the launching environment is kept.
    // GDK picks Wayland when a Wayland display is set; some sessions (WSLg)
    // set no XDG_SESSION_TYPE.
    let wayland_display =
        std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
    let backend = if std::env::var_os(GDK_BACKEND).is_none()
        && (wayland_display || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland"))
        && std::env::var_os("DISPLAY").is_some()
    {
        if data_dir().is_some_and(|dir| read_float_window_from_config(&dir)) {
            set_variables(&BTreeMap::from([(GDK_BACKEND, "x11".to_owned())]));
            guard_log("gpu_guard: floating window on Wayland, running under XWayland (tao#1134)");
            FloatBackend::XWayland
        } else {
            FloatBackend::Wayland
        }
    } else {
        FloatBackend::Given
    };
    let _ = FLOAT_BACKEND.set(backend);
    // NVIDIA's EGL Wayland path can enable explicit sync before GTK supplies
    // an acquire point. Disable only that driver feature.
    let wayland = std::env::var(GDK_BACKEND).map_or_else(
        |_| std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland"),
        |backend| backend == "wayland",
    );
    let nvidia = nvidia_driver_loaded();
    if wayland && nvidia && std::env::var_os(NV_DISABLE_EXPLICIT_SYNC).is_none() {
        set_variables(&BTreeMap::from([(
            NV_DISABLE_EXPLICIT_SYNC,
            "1".to_owned(),
        )]));
        guard_log("gpu_guard: disabled NVIDIA Wayland explicit sync (WebKit #324551)");
    }
    let external = [
        WEBKIT_DISABLE_DMABUF_RENDERER,
        WEBKIT_DISABLE_COMPOSITING_MODE,
        WEBKIT_DMABUF_RENDERER_FORCE_SHM,
    ]
    .into_iter()
    .filter_map(|key| Some(format!("{key}={:?}", std::env::var_os(key)?)))
    .collect::<Vec<_>>();
    if !external.is_empty() {
        guard_log(&format!(
            "gpu_guard: external rendering overrides {}",
            external.join(" ")
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
            guard_log(
                "gpu_guard: no Mesa EGL; disabled the DMA-BUF renderer for NVIDIA (tauri#13493)",
            );
        } else {
            guard_log("gpu_guard: CPU painting with SHM compositing over Mesa EGL");
        }
        set_variables(&variables);
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
    // Which of the rendering variables this process set itself, and why.
    let own = std::env::var(SELF_SET_MARKER).unwrap_or_default();
    let own_rendering = inherited_variables(&own)
        .into_iter()
        .any(|key| key != GDK_BACKEND && key != NV_DISABLE_EXPLICIT_SYNC);
    let software = data_dir().is_some_and(|dir| read_software_rendering_from_config(&dir));
    let disable_override_source = if own_rendering && software {
        "software_rendering_preference"
    } else if own_rendering {
        "nvidia_driver"
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
    fn a_relaunch_clears_only_the_variables_its_parent_set() {
        assert_eq!(
            inherited_variables("GDK_BACKEND,LIBGL_ALWAYS_SOFTWARE,PATH,HOME"),
            [GDK_BACKEND, LIBGL_ALWAYS_SOFTWARE]
        );
        assert!(inherited_variables("").is_empty());
        // Earlier builds named only why they set their variables.
        assert_eq!(
            inherited_variables("software"),
            [
                WEBKIT_DISABLE_DMABUF_RENDERER,
                WEBKIT_DISABLE_COMPOSITING_MODE
            ]
        );
        assert_eq!(
            inherited_variables(NVIDIA_DRIVER),
            [WEBKIT_DISABLE_DMABUF_RENDERER]
        );
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
