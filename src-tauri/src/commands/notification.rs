use crate::error::AppError;
use crate::services::notification::{
    notification_access, send_app_notification, NotificationAccess,
};
use tauri::AppHandle;

#[tauri::command]
pub async fn send_app_system_notification(
    app: AppHandle,
    title: String,
    body: String,
) -> Result<(), AppError> {
    send_app_notification(&app, &title, &body).await
}

/// Whether the system shows this app's notifications. Asking may block on
/// the system, so it runs off the main thread.
#[tauri::command]
pub async fn notification_access_state(app: AppHandle) -> NotificationAccess {
    tauri::async_runtime::spawn_blocking(move || notification_access(&app))
        .await
        .unwrap_or(NotificationAccess::Unknown)
}

/// Opens the system's notification settings.
#[tauri::command]
pub fn open_notification_settings(app: AppHandle) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let url = if cfg!(windows) {
        "ms-settings:notifications"
    } else if cfg!(target_os = "macos") {
        "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
    } else {
        return Err(AppError::NotFound("system notification settings".into()));
    };
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| AppError::Io(error.to_string()))
}
