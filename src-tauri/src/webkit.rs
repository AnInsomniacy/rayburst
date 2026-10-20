//! WebKitGTK setup on Linux.
//!
//! Hardware rendering is opt-in. Software mode keeps Mesa's compositor alive
//! with CPU painting and shared-memory frames: WebKitGTK 2.50.4's
//! non-composited backing store accumulates translucent shadows instead of
//! replacing them. Hardware mode also uses shared-memory frame transport:
//! distro NVIDIA blocklists otherwise disable compositing, while forcing the
//! DMA-BUF path can fail to allocate GBM buffers. SHM retains the native GL
//! compositor without forcing CPU painting or a different graphics vendor.
//! Whole-frame repaints also prevent incomplete frames on WebKitGTK 2.54
//! (#650, #705).

#[cfg(target_os = "linux")]
use std::{collections::BTreeMap, ffi::OsString, path::Path};

#[cfg(target_os = "linux")]
const DISABLE_DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
#[cfg(target_os = "linux")]
const FORCE_DMABUF: &str = "WEBKIT_FORCE_DMABUF_RENDERER";
#[cfg(target_os = "linux")]
const DISABLE_COMPOSITING: &str = "WEBKIT_DISABLE_COMPOSITING_MODE";
#[cfg(target_os = "linux")]
const FORCE_SHM: &str = "WEBKIT_DMABUF_RENDERER_FORCE_SHM";
#[cfg(target_os = "linux")]
const CPU_RENDERING: &str = "WEBKIT_SKIA_ENABLE_CPU_RENDERING";
#[cfg(target_os = "linux")]
const SOFTWARE_GL: &str = "LIBGL_ALWAYS_SOFTWARE";
#[cfg(target_os = "linux")]
const EGL_VENDOR: &str = "__EGL_VENDOR_LIBRARY_FILENAMES";

/// Applies the `hardwareRendering` preference. The preference decides, over
/// what a previous run of the app may have left in the environment. Must run
/// before any other thread starts.
pub fn prepare() {
    #[cfg(target_os = "linux")]
    {
        let hardware = crate::startup_preference("hardwareRendering")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mesa = mesa_egl_vendor(&[
            Path::new("/etc/glvnd/egl_vendor.d"),
            Path::new("/usr/share/glvnd/egl_vendor.d"),
        ]);
        if !hardware && mesa.is_none() {
            eprintln!("[rayburst] webkit: Mesa EGL unavailable; legacy software rendering may retain transparent frames");
        }
        for (name, value) in rendering_environment(hardware, mesa.as_deref()) {
            // SAFETY: called first in `run`, before Tauri starts any thread.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn mesa_egl_vendor(directories: &[&Path]) -> Option<std::path::PathBuf> {
    directories.iter().find_map(|directory| {
        let mut paths: Vec<_> = std::fs::read_dir(directory)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect();
        paths.sort();
        paths.into_iter().find(|path| {
            let library = (|| -> Option<String> {
                let json: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
                Some(json.get("ICD")?.get("library_path")?.as_str()?.into())
            })();
            library.is_some_and(|library| library.contains("libEGL_mesa"))
        })
    })
}

/// Every managed variable has an explicit value or removal, so switching the
/// preference on a relaunch cannot inherit the previous rendering mode.
#[cfg(target_os = "linux")]
fn rendering_environment(
    hardware: bool,
    mesa_vendor: Option<&Path>,
) -> BTreeMap<&'static str, Option<OsString>> {
    let mut variables = [
        DISABLE_DMABUF,
        FORCE_DMABUF,
        DISABLE_COMPOSITING,
        FORCE_SHM,
        CPU_RENDERING,
        SOFTWARE_GL,
        EGL_VENDOR,
    ]
    .into_iter()
    .map(|name| (name, None))
    .collect::<BTreeMap<_, _>>();
    if hardware {
        // The user explicitly opted in. Bypass distro NVIDIA blocklists, but
        // avoid GBM buffer allocation/import failures by transporting complete
        // frames over SHM. Keep the vendor and painting strategy at defaults.
        variables.insert(FORCE_DMABUF, Some("1".into()));
        variables.insert(FORCE_SHM, Some("1".into()));
        return variables;
    }
    let Some(mesa_vendor) = mesa_vendor else {
        // Preserve the crash fallback on systems without a software EGL vendor.
        variables.insert(DISABLE_DMABUF, Some("1".into()));
        return variables;
    };
    for name in [FORCE_SHM, CPU_RENDERING, SOFTWARE_GL] {
        variables.insert(name, Some("1".into()));
    }
    // Keep the display server's GLX vendor. Forcing Mesa GLX on NVIDIA can
    // prevent GTK from creating an RGBA context and disable compositing again.
    variables.insert(EGL_VENDOR, Some(mesa_vendor.as_os_str().into()));
    variables
}

/// WebKit feature that limits compositing to a frame's damaged rectangles.
#[cfg(target_os = "linux")]
const DAMAGE_COMPOSITING: &str = "UseDamagingInformationForCompositing";

#[cfg(target_os = "linux")]
pub fn configure_webview<R: tauri::Runtime>(webview: &tauri::Webview<R>) {
    let enabled = |name| std::env::var(name).is_ok_and(|value| value != "0");
    let shm = enabled(FORCE_SHM) && !enabled(DISABLE_DMABUF) && !enabled(DISABLE_COMPOSITING);
    let label = webview.label().to_owned();
    let configured = webview.with_webview(move |platform| {
        use webkit2gtk::{
            glib::object::ObjectType, HardwareAccelerationPolicy, SettingsExt, WebViewExt,
        };
        if let Some(settings) = platform.inner().settings() {
            if shm {
                // Keep replacing complete frames after CSS animations finish.
                settings.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
            }
            // SAFETY: the settings are alive for the call, on the GTK thread
            // that owns them.
            unsafe { features::disable(settings.as_ptr().cast(), DAMAGE_COMPOSITING) };
            let policy = settings.hardware_acceleration_policy();
            log::info!("webkit: webview={label} acceleration_policy={policy:?} shm={shm}");
            if shm && policy == HardwareAccelerationPolicy::Never {
                log::warn!("webkit: webview={label} compositing unavailable despite Always policy; transparent frames may accumulate");
            }
        }
    });
    if let Err(error) = configured {
        log::warn!("webkit: webview not configured error={error}");
    }
}

/// Reads the existing main WebView on its GTK thread, including WebKit's
/// capability fallback. A policy still does not prove active GPU painting.
#[cfg(target_os = "linux")]
async fn acceleration_policy(app: &tauri::AppHandle) -> Result<String, crate::error::AppError> {
    use crate::error::AppError;
    use tauri::Manager;
    use webkit2gtk::{SettingsExt, WebViewExt};

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::NotFound("The main WebView is not available".into()))?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window
        .with_webview(move |platform| {
            let policy = platform
                .inner()
                .settings()
                .map(|settings| format!("{:?}", settings.hardware_acceleration_policy()))
                .ok_or_else(|| AppError::NotFound("WebKitGTK settings are not available".into()));
            let _ = sender.send(policy);
        })
        .map_err(|error| AppError::Io(format!("Failed to inspect the WebView: {error}")))?;
    tokio::time::timeout(std::time::Duration::from_secs(2), receiver)
        .await
        .map_err(|_| AppError::Io("Timed out reading the WebKitGTK policy".into()))?
        .map_err(|_| AppError::Io("The WebView closed before its policy could be read".into()))?
}

/// Records requested rendering separately from WebKit's effective policy.
pub async fn diagnostics(_app: &tauri::AppHandle) -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        let variable = |name| std::env::var(name).ok();
        let policy = acceleration_policy(_app).await;
        serde_json::json!({
            "webkitgtk_version": tauri::webview_version().ok(),
            "hardware_rendering_requested": variable(FORCE_DMABUF).as_deref() == Some("1"),
            "hardware_acceleration_policy": policy.as_ref().ok(),
            "policy_error": policy.err().map(|error| error.to_string()),
            "rendering_environment": {
                DISABLE_DMABUF: variable(DISABLE_DMABUF),
                FORCE_DMABUF: variable(FORCE_DMABUF),
                DISABLE_COMPOSITING: variable(DISABLE_COMPOSITING),
                FORCE_SHM: variable(FORCE_SHM),
                CPU_RENDERING: variable(CPU_RENDERING),
                SOFTWARE_GL: variable(SOFTWARE_GL),
                EGL_VENDOR: variable(EGL_VENDOR),
            },
            "GDK_BACKEND": variable("GDK_BACKEND"),
            "XDG_SESSION_TYPE": variable("XDG_SESSION_TYPE"),
        })
    }
    #[cfg(not(target_os = "linux"))]
    serde_json::Value::Null
}

/// WebKitGTK's feature API (2.42+), looked up at run time: binding it at link
/// time would keep the app from starting on an older WebKitGTK.
#[cfg(target_os = "linux")]
mod features {
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

    /// Turns `identifier` off in `settings`, when this WebKitGTK has both.
    ///
    /// # Safety
    /// `settings` must point to a live `WebKitSettings`.
    pub unsafe fn disable(settings: *mut c_void, identifier: &str) {
        let (Some(all), Some(length), Some(get), Some(unref), Some(id), Some(set)) = (
            symbol::<AllFeatures>(c"webkit_settings_get_all_features"),
            symbol::<ListLength>(c"webkit_feature_list_get_length"),
            symbol::<ListGet>(c"webkit_feature_list_get"),
            symbol::<ListUnref>(c"webkit_feature_list_unref"),
            symbol::<Identifier>(c"webkit_feature_get_identifier"),
            symbol::<SetEnabled>(c"webkit_settings_set_feature_enabled"),
        ) else {
            return;
        };
        let list = all();
        if list.is_null() {
            return;
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
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn software_frames_keep_the_compositor_and_use_cpu_mesa() {
        let vendor = Path::new("/usr/share/glvnd/egl_vendor.d/50_mesa.json");
        let variables = rendering_environment(false, Some(vendor));
        for name in [DISABLE_DMABUF, DISABLE_COMPOSITING, FORCE_DMABUF] {
            assert_eq!(
                variables[name], None,
                "{name} must not disable frame replacement"
            );
        }
        for name in [FORCE_SHM, CPU_RENDERING, SOFTWARE_GL] {
            assert_eq!(variables[name].as_deref(), Some(std::ffi::OsStr::new("1")));
        }
        assert_eq!(variables[EGL_VENDOR].as_deref(), Some(vendor.as_os_str()));
        assert!(!variables.contains_key("__GLX_VENDOR_LIBRARY_NAME"));
    }

    #[test]
    fn hardware_mode_keeps_shm_compositing_without_forcing_cpu_or_mesa() {
        let vendor = Path::new("/usr/share/glvnd/egl_vendor.d/50_mesa.json");
        let software = rendering_environment(false, Some(vendor));
        let hardware = rendering_environment(true, Some(vendor));
        assert_eq!(
            software.keys().collect::<Vec<_>>(),
            hardware.keys().collect::<Vec<_>>()
        );
        for name in [FORCE_DMABUF, FORCE_SHM] {
            assert_eq!(hardware[name].as_deref(), Some(std::ffi::OsStr::new("1")));
        }
        for name in [
            DISABLE_DMABUF,
            DISABLE_COMPOSITING,
            CPU_RENDERING,
            SOFTWARE_GL,
            EGL_VENDOR,
        ] {
            assert_eq!(hardware[name], None);
        }
        assert_eq!(hardware, rendering_environment(true, None));
    }

    #[test]
    fn missing_mesa_keeps_the_legacy_fallback_and_clears_shm_overrides() {
        let variables = rendering_environment(false, None);
        assert_eq!(
            variables[DISABLE_DMABUF].as_deref(),
            Some(std::ffi::OsStr::new("1"))
        );
        for name in [
            FORCE_SHM,
            FORCE_DMABUF,
            CPU_RENDERING,
            SOFTWARE_GL,
            EGL_VENDOR,
            DISABLE_COMPOSITING,
        ] {
            assert_eq!(variables[name], None);
        }
    }

    #[test]
    fn mesa_discovery_reads_vendor_metadata_and_skips_invalid_entries() {
        let directory = tempfile::tempdir().expect("temporary vendor directory");
        let missing = directory.path().join("missing");
        let vendor = directory.path().join("custom-mesa.json");
        std::fs::write(directory.path().join("00-broken.json"), "{").unwrap();
        std::fs::write(
            directory.path().join("10-nvidia.json"),
            r#"{"ICD":{"library_path":"libEGL_nvidia.so.0"}}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("20-invalid.json"), r#"{"ICD":{}}"#).unwrap();
        assert_eq!(mesa_egl_vendor(&[&missing, directory.path()]), None);
        std::fs::write(&vendor, r#"{"ICD":{"library_path":"libEGL_mesa.so.0"}}"#).unwrap();
        assert_eq!(mesa_egl_vendor(&[&missing, directory.path()]), Some(vendor));
    }
}
