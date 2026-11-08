//! Native protocol associations. The application activation scheme repairs itself;
//! public download schemes change only in response to an explicit user action.
use crate::error::AppError;
use tauri::AppHandle;

// ── macOS native implementation ─────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos;

// Query the effective Shell association, not an application-owned registry key.
#[cfg(windows)]
mod windows;

// ── Linux native associations ──────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
async fn with_linux_associations<T: Send + 'static>(
    app: &AppHandle,
    operation: impl FnOnce(&linux::Associations) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    use tauri::Manager;

    let executable = match app.env().appimage {
        Some(path) => std::path::PathBuf::from(path),
        None => tauri::utils::platform::current_exe()?,
    };
    tokio::task::spawn_blocking(move || {
        // Serialize read/modify/write operations; GIO objects stay on this worker.
        static ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ACCESS
            .lock()
            .map_err(|error| AppError::Protocol(error.to_string()))?;
        operation(&linux::Associations::new(&executable)?)
    })
    .await
    .map_err(|error| AppError::Protocol(error.to_string()))?
}

#[cfg(target_os = "linux")]
pub(crate) async fn protocol_diagnostics(app: &AppHandle) -> serde_json::Value {
    match with_linux_associations(app, |associations| Ok(associations.diagnostics())).await {
        Ok(snapshot) => snapshot,
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssociationStatus {
    state: &'static str,
    handler: Option<String>,
    error: Option<String>,
    can_change: bool,
}

#[tauri::command]
pub async fn get_association_status(
    app: AppHandle,
    protocol: String,
) -> Result<AssociationStatus, AppError> {
    validate_protocol(&protocol)?;
    let can_change = !tauri::is_dev();
    #[cfg(windows)]
    let result = windows::handler(&protocol).map(|handler| {
        let current = std::env::current_exe().ok();
        let state = match &handler.path {
            None if handler.unavailable => "unavailable",
            None => "unassigned",
            Some(path) if !path.is_file() => "unavailable",
            Some(path)
                if current.as_ref().is_some_and(|expected| {
                    dunce::simplified(path)
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&dunce::simplified(expected).to_string_lossy())
                }) =>
            {
                "current"
            }
            Some(_) => "other",
        };
        (
            state,
            handler.path.map(|path| path.to_string_lossy().into_owned()),
        )
    });
    #[cfg(target_os = "macos")]
    let result = Ok::<_, AppError>(match macos::get_default_handler_bundle_id(&protocol) {
        Some(handler) => (
            if handler == app.config().identifier {
                "current"
            } else {
                "other"
            },
            Some(handler),
        ),
        None => ("unassigned", None),
    });
    #[cfg(target_os = "linux")]
    let result =
        with_linux_associations(&app, move |associations| associations.status(&protocol)).await;
    let _ = app;
    Ok(match result {
        Ok((state, handler)) => AssociationStatus {
            state,
            handler,
            error: None,
            can_change,
        },
        Err(error) => AssociationStatus {
            state: "error",
            handler: None,
            error: Some(error.to_string()),
            can_change,
        },
    })
}

// ── Cross-platform Tauri commands ───────────────────────────────────

/// Queries the activation association on platforms that repair it at runtime.
#[cfg(any(windows, target_os = "linux"))]
async fn is_default_protocol_client(app: AppHandle, protocol: String) -> Result<bool, AppError> {
    validate_protocol(&protocol)?;
    #[cfg(windows)]
    {
        let _ = &app;
        windows::is_default(&protocol)
    }
    #[cfg(target_os = "linux")]
    {
        with_linux_associations(&app, move |associations| associations.is_default(&protocol)).await
    }
}

/// Registers this application as the OS-level default handler for the
/// given URL scheme.
#[tauri::command]
pub async fn set_default_protocol_client(app: AppHandle, protocol: String) -> Result<(), AppError> {
    validate_protocol_change(&protocol)?;
    #[cfg(target_os = "macos")]
    {
        let bundle_id = &app.config().identifier;
        macos::set_as_default_handler(&protocol, bundle_id).map_err(AppError::Protocol)?;

        // Verify the registration actually took effect.
        let handler = macos::get_default_handler_bundle_id(&protocol);
        let registered = handler.as_deref() == Some(bundle_id.as_str());
        if registered {
            Ok(())
        } else {
            Err(AppError::Protocol(format!(
                "registration accepted but did not take effect (handler={handler:?}, expected={bundle_id})"
            )))
        }
    }
    #[cfg(windows)]
    {
        // The installer owns public candidates. Runtime repair only owns the
        // private activation scheme; it must not duplicate installed capabilities.
        if protocol == "rayburst" && !windows::is_default(&protocol).unwrap_or(false) {
            use tauri_plugin_deep_link::DeepLinkExt;
            app.deep_link()
                .register(&protocol)
                .map_err(|error| AppError::Protocol(error.to_string()))?;
        }
        windows::notify_changed();
        if windows::is_default(&protocol)? {
            Ok(())
        } else {
            // Protected user choices require an explicit visit to Settings.
            Err(AppError::Protocol("manual_change_required".into()))
        }
    }
    #[cfg(target_os = "linux")]
    {
        with_linux_associations(&app, move |associations| {
            associations.set_default(&protocol)
        })
        .await
    }
}

fn validate_protocol(protocol: &str) -> Result<(), AppError> {
    if matches!(
        protocol,
        ".torrent" | "rayburst" | "magnet" | "ed2k" | "thunder"
    ) {
        Ok(())
    } else {
        Err(AppError::Protocol("Unsupported protocol".into()))
    }
}

fn validate_protocol_change(protocol: &str) -> Result<(), AppError> {
    validate_protocol(protocol)?;
    if tauri::is_dev() {
        return Err(AppError::Protocol(
            "Protocol registration is disabled in development mode. Use an installed build.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn repair_activation_protocol(app: &AppHandle) {
    if tauri::is_dev() {
        return;
    }
    #[cfg(any(windows, target_os = "linux"))]
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = async {
                #[cfg(target_os = "linux")]
                if let Err(error) = with_linux_associations(&app, |associations| {
                    associations.tidy();
                    Ok(())
                })
                .await
                {
                    log::warn!("protocol:tidy-failed error={error}");
                }
                match is_default_protocol_client(app.clone(), "rayburst".into()).await {
                    Ok(true) => return Ok(()),
                    Ok(false) => {}
                    Err(error) => log::warn!("protocol:activation-query-failed error={error}"),
                }
                // A broken association can fail the query itself. Registration
                // remains idempotent and its result is verified by the Shell.
                #[cfg(windows)]
                {
                    use tauri_plugin_deep_link::DeepLinkExt;
                    app.deep_link()
                        .register("rayburst")
                        .map_err(|error| AppError::Protocol(error.to_string()))?;
                    windows::notify_changed();
                    if !windows::is_default("rayburst")? {
                        return Err(AppError::Protocol("manual_change_required".into()));
                    }
                }
                #[cfg(target_os = "linux")]
                set_default_protocol_client(app.clone(), "rayburst".into()).await?;
                log::info!("protocol:activation-repaired");
                Ok::<_, AppError>(())
            }
            .await;
            if let Err(error) = result {
                log::warn!("protocol:activation-repair-failed error={error}");
            }
        });
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = app;
}

#[cfg(windows)]
pub(crate) async fn protocol_diagnostics(_app: &AppHandle) -> serde_json::Value {
    let mut protocols = serde_json::Map::new();
    for scheme in [".torrent", "rayburst", "magnet", "ed2k", "thunder"] {
        let snapshot = match windows::handler(scheme) {
            Ok(handler) => serde_json::json!({
                "handler": handler.path,
                "unavailable": handler.unavailable,
                "currentApplication": windows::is_default(scheme).ok(),
            }),
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        };
        protocols.insert(scheme.into(), snapshot);
    }
    serde_json::Value::Object(protocols)
}

#[cfg(target_os = "macos")]
pub(crate) async fn protocol_diagnostics(app: &AppHandle) -> serde_json::Value {
    let mut associations = serde_json::Map::new();
    for scheme in [".torrent", "rayburst", "magnet", "ed2k", "thunder"] {
        let snapshot = match get_association_status(app.clone(), scheme.into()).await {
            Ok(status) => serde_json::json!(status),
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        };
        associations.insert(scheme.into(), snapshot);
    }
    serde_json::Value::Object(associations)
}

#[tauri::command]
pub fn open_default_apps_settings(app: AppHandle) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(windows::settings_url(&app)?, None::<String>)
            .map_err(|error| AppError::Protocol(error.to_string()))
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(AppError::Protocol(
            "Default Apps settings are only available on Windows".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::validate_protocol_change;

    #[test]
    fn protocol_changes_require_a_production_build_and_supported_scheme() {
        for scheme in [".torrent", "rayburst", "magnet", "ed2k", "thunder"] {
            let result = validate_protocol_change(scheme);
            if tauri::is_dev() {
                assert!(result.unwrap_err().to_string().contains("development mode"));
            } else {
                assert!(result.is_ok());
            }
        }
        assert!(validate_protocol_change("https").is_err());
    }
}
