//! Browser download handoff and native task creation.
pub mod category;
pub mod contracts;
mod handoff;
mod native;
pub(crate) mod preferences;

use crate::aria2::types::Aria2Probe;
use crate::{error::AppError, services::tasks::TaskServiceState};
use contracts::{AddRequest, AddResponse, FallbackReason};
pub use handoff::Handoffs;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// How long a confirmation dialog may hold a browser download. After that
/// the browser keeps the download and the dialog drops it.
const CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(120);

/// The desktop filename field never grants permission to select a directory.
pub fn validate_filename(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Ok(());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if name == "."
        || name == ".."
        || reserved
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|ch| ch.is_control() || "\\/:<>\"|?*".contains(ch))
    {
        return Err(AppError::InvalidInput(
            "Enter a filename without a path or reserved characters".into(),
        ));
    }
    Ok(())
}

pub async fn dispatch(app: &AppHandle, request: AddRequest) -> Result<AddResponse, AppError> {
    let url = request.url.as_str();
    let parsed = url::Url::parse(url).ok();
    let engine_link = ["ed2k://", "thunder://"]
        .iter()
        .any(|prefix| url.to_ascii_lowercase().starts_with(prefix));
    if !engine_link
        && !parsed
            .as_ref()
            .is_some_and(|url| matches!(url.scheme(), "http" | "https" | "sftp" | "magnet"))
    {
        return Err(AppError::InvalidInput("Unsupported download URL".into()));
    }
    let prefs = preferences::load(app)?;
    crate::commands::database_initialize(app.clone()).await?;
    let options = preferences::options(&prefs, &request)?;
    if request.verify
        && parsed
            .as_ref()
            .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
    {
        let engine = app.state::<TaskServiceState>().0.clone();
        let probe = engine.probe(url, options.clone()).await?;
        if let Some(reason) = refusal(&probe, &request) {
            log::info!(
                "downloads: browser keeps the download ({reason:?}): status={} type={:?} size={:?} error={:?}",
                probe.status,
                probe.content_type,
                probe.total_length,
                probe.error
            );
            return Ok(AddResponse::fallback(reason));
        }
        log::info!(
            "downloads: engine verified the download: status={} size={:?} resumable={} redirected={}",
            probe.status,
            probe.total_length,
            probe.resumable,
            probe.final_url != url
        );
    }
    // Torrent files need native metainfo inspection and a file-selection dialog.
    if prefs.extension_download_behavior == preferences::ExtensionDownloadBehavior::Confirm
        || parsed
            .as_ref()
            .is_some_and(|url| url.path().to_ascii_lowercase().ends_with(".torrent"))
    {
        return confirm(app, &request).await;
    }
    let gid = submit(app, TaskInput::Uris(vec![url.to_owned()]), options, None).await?;
    if prefs.extension_download_behavior == preferences::ExtensionDownloadBehavior::Show {
        super::frontend_action::dispatch_frontend_action(
            app,
            super::frontend_action::FrontendActionChannel::TrayMenuAction,
            super::frontend_action::FrontendActionKind::ShowDownloads,
            "http-api",
        );
    }
    Ok(AddResponse::added(gid))
}

/// Why the engine must not take over what the browser is downloading.
fn refusal(probe: &Aria2Probe, request: &AddRequest) -> Option<FallbackReason> {
    if probe.status == "0" {
        return Some(FallbackReason::Unreachable);
    }
    if probe.error.is_some() || !probe.status.starts_with('2') {
        return Some(FallbackReason::Rejected);
    }
    let page = |mime: &str| {
        let essence = mime.split(';').next().unwrap_or_default().trim();
        essence.eq_ignore_ascii_case("text/html")
            || essence.eq_ignore_ascii_case("application/xhtml+xml")
    };
    if page(&probe.content_type) && !request.mime.as_deref().is_some_and(page) {
        return Some(FallbackReason::WebPage);
    }
    let size = probe
        .total_length
        .as_deref()
        .and_then(|length| length.parse::<i64>().ok());
    match (request.size.filter(|size| *size > 0), size) {
        (Some(expected), Some(actual)) if expected != actual => Some(FallbackReason::SizeMismatch),
        _ => None,
    }
}

/// Holds the browser download while the user decides in the add dialog.
async fn confirm(app: &AppHandle, request: &AddRequest) -> Result<AddResponse, AppError> {
    let handoffs = app.state::<Handoffs>();
    let emitter = app.clone();
    let pending = handoffs.open(Box::new(move |id| {
        let _ = emitter.emit(handoff::EXPIRED_EVENT, serde_json::json!({ "id": id }));
    }));
    show_confirmation(app, request, pending.id());
    Ok(match pending.wait(CONFIRMATION_TIMEOUT).await {
        Some(handoff::Outcome::Added(gid)) => AddResponse::added(gid),
        Some(handoff::Outcome::Cancelled) => AddResponse::cancelled(),
        None => AddResponse::fallback(FallbackReason::Unconfirmed),
    })
}

pub enum TaskInput {
    Uris(Vec<String>),
    Torrent(String),
}
impl TaskInput {
    async fn create(
        self,
        engine: &crate::services::tasks::TaskService,
        mut options: serde_json::Value,
    ) -> Result<String, AppError> {
        let options_map = options
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidInput("Download options must be an object".into()))?;
        let gid = options_map
            .entry("gid")
            .or_insert_with(|| {
                uuid::Uuid::new_v4().simple().to_string()[..16]
                    .to_string()
                    .into()
            })
            .as_str()
            .ok_or_else(|| AppError::InvalidInput("Invalid task ID".into()))?
            .to_string();
        engine.tasks.expect_start(&gid).await;
        let result = match self {
            Self::Uris(uris) => engine.add_uri(uris, options).await,
            Self::Torrent(torrent) => engine.add_torrent(&torrent, options).await,
        };
        if matches!(
            result,
            Err(AppError::Rpc { .. } | AppError::InvalidInput(_))
        ) {
            engine.tasks.take_start(&gid).await;
        }
        result
    }
}

/// Creates a task. `request_id` names the browser download the add dialog
/// confirms; it answers that download once the task exists.
pub async fn submit(
    app: &AppHandle,
    input: TaskInput,
    mut options: serde_json::Value,
    request_id: Option<&str>,
) -> Result<String, AppError> {
    let engine = app.state::<TaskServiceState>().0.clone();
    let handoffs = app.state::<Handoffs>();
    let claim = request_id.map(|id| handoffs.claim(id)).transpose()?;
    preferences::validate_save_location(app, &options)?;
    if let TaskInput::Uris(uris) = &input {
        preferences::apply_site_rules(&preferences::load(app)?, uris, &mut options);
    }
    let automatic = if let TaskInput::Uris(uris) = &input {
        native::prepare(app, &engine, uris, &mut options).await?
    } else {
        None
    };
    let generation = engine.generation();
    let selection_gid = options
        .get("gid")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let _mutation = engine.mutation.lock().await;
    let result = input.create(&engine, options).await;
    match &result {
        Ok(gid) => {
            if let Some(claim) = claim {
                claim.finish(gid.clone());
            }
            super::pump::wake(app);
        }
        Err(AppError::Rpc { .. } | AppError::InvalidInput(_)) => {
            if let Some(gid) = selection_gid {
                app.state::<crate::database::DatabaseState>()
                    .0
                    .remove_bt_selection(&gid)
                    .await?;
            }
        }
        Err(_) => {}
    }
    drop(_mutation);
    native::finish(app.clone(), engine, automatic, generation).await;
    result
}

fn show_confirmation(app: &AppHandle, request: &AddRequest, id: &str) {
    let input = crate::services::external_input::ExternalDownloadInput {
        request_id: Some(id.to_owned()),
        filename_source: Some(request.filename_source),
        url: request.url.clone(),
        final_url: request.final_url.clone(),
        referer: request.referer.clone(),
        cookie: request.cookie.clone(),
        filename: request.filename.clone(),
        user_agent: request.user_agent.clone(),
        request_headers: request.request_headers.clone(),
        source: Some("http-api".into()),
    };
    crate::services::external_input::route_external_inputs(app, vec![input], "http-api", false);
}

/// The user declined a browser download in the add dialog.
pub fn cancel(app: &AppHandle, id: &str) {
    app.state::<Handoffs>().cancel(id);
}

#[cfg(test)]
mod tests;
