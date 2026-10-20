//! WebKitGTK setup on Linux.
//!
//! The compositor always runs and hands frames over in shared memory. Turning
//! DMA-BUF off instead turns compositing off, and WebKitGTK 2.50 then leaves
//! translucent shadows and old frames on screen (#731); distribution builds
//! also turn it off for NVIDIA unless forced. Painting stays on the CPU and
//! Mesa's software GL unless the user turns GPU acceleration on: on many
//! drivers and virtual GPUs the GPU path freezes windows or leaves them blank.
//! Every webview also repaints whole frames: WebKitGTK 2.54 composites only
//! the damaged regions of a frame, and the rest of the window stays blank
//! (#650, #705).

#[cfg(target_os = "linux")]
const COMPOSITING: [&str; 2] = [
    "WEBKIT_FORCE_DMABUF_RENDERER",
    "WEBKIT_DMABUF_RENDERER_FORCE_SHM",
];
#[cfg(target_os = "linux")]
const SOFTWARE: [&str; 2] = ["WEBKIT_SKIA_ENABLE_CPU_RENDERING", "LIBGL_ALWAYS_SOFTWARE"];

/// Applies the `hardwareRendering` preference. The preference decides, over
/// what a previous run of the app may have left in the environment. Must run
/// before any other thread starts.
pub fn prepare() {
    #[cfg(target_os = "linux")]
    {
        let hardware = crate::startup_preference("hardwareRendering")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        // SAFETY: called first in `run`, before Tauri starts any thread.
        unsafe {
            std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");
            for name in COMPOSITING {
                std::env::set_var(name, "1");
            }
            for name in SOFTWARE {
                if hardware {
                    std::env::remove_var(name);
                } else {
                    std::env::set_var(name, "1");
                }
            }
        }
    }
}

/// WebKit feature that limits compositing to a frame's damaged rectangles.
#[cfg(target_os = "linux")]
const DAMAGE_COMPOSITING: &str = "UseDamagingInformationForCompositing";

#[cfg(target_os = "linux")]
pub fn configure_webview<R: tauri::Runtime>(webview: &tauri::Webview<R>) {
    let configured = webview.with_webview(|platform| {
        use webkit2gtk::{glib::object::ObjectType, WebViewExt};
        if let Some(settings) = platform.inner().settings() {
            // SAFETY: the settings are alive for the call, on the GTK thread
            // that owns them.
            unsafe { features::disable(settings.as_ptr().cast(), DAMAGE_COMPOSITING) };
        }
    });
    if let Err(error) = configured {
        log::warn!("webkit: webview not configured error={error}");
    }
}

/// What a bug report needs to know about the rendering setup.
pub fn diagnostics() -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        let variable = |name| std::env::var(name).ok();
        serde_json::json!({
            "webkitgtk_version": tauri::webview_version().ok(),
            "gpu_acceleration": variable(SOFTWARE[0]).is_none(),
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
