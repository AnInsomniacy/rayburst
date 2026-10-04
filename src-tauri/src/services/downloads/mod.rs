//! Download intent, submission receipts and native task creation.
pub mod category;
pub mod contracts;
mod native;
mod preferences;

use crate::database::SubmissionState;
use crate::{database::DatabaseState, error::AppError, services::tasks::TaskServiceState};
use contracts::{AddRequest, AddResponse};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct SubmissionGate(Mutex<()>);

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
    if request.id.is_empty()
        || request.id.len() > 128
        || !request
            .id
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-' || ch == b':')
    {
        return Err(AppError::InvalidInput("Invalid download request ID".into()));
    }
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
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&request)?));
    let record = app
        .state::<DatabaseState>()
        .0
        .reserve_submission(&request.id, &fingerprint)
        .await?;
    if record.state == SubmissionState::Submitted {
        return Ok(AddResponse {
            id: request.id,
            action: "submitted",
            gid: Some(record.gid),
        });
    }
    if record.state == SubmissionState::Cancelled {
        return Ok(AddResponse {
            id: request.id,
            action: "cancelled",
            gid: None,
        });
    }
    // Torrent files need native metainfo inspection and a file-selection dialog.
    if record.state == SubmissionState::Pending
        && prefs.extension_download_behavior != preferences::ExtensionDownloadBehavior::Confirm
        && !parsed
            .as_ref()
            .is_some_and(|url| url.path().to_ascii_lowercase().ends_with(".torrent"))
    {
        let options = preferences::options(&prefs, &request)?;
        let gid = submit(
            app,
            TaskInput::Uris(vec![url.to_owned()]),
            options,
            Some(&request.id),
        )
        .await?;
        if prefs.extension_download_behavior == preferences::ExtensionDownloadBehavior::Show {
            super::frontend_action::dispatch_frontend_action(
                app,
                super::frontend_action::FrontendActionChannel::TrayMenuAction,
                super::frontend_action::FrontendActionKind::ShowDownloads,
                "http-api",
            );
        }
        return Ok(AddResponse {
            id: request.id,
            action: "submitted",
            gid: Some(gid),
        });
    }
    app.state::<DatabaseState>()
        .0
        .stage_confirmation(&request.id, &serde_json::to_string(&request)?)
        .await?;
    show_confirmation(app, &request);
    Ok(AddResponse {
        id: request.id,
        action: "needs-confirmation",
        gid: None,
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

pub async fn submit(
    app: &AppHandle,
    input: TaskInput,
    mut options: serde_json::Value,
    request_id: Option<&str>,
) -> Result<String, AppError> {
    let engine = app.state::<TaskServiceState>().0.clone();
    if let Some(id) = request_id {
        let record = app
            .state::<DatabaseState>()
            .0
            .submission(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Download request expired".into()))?;
        if record.state == SubmissionState::Submitted {
            return Ok(record.gid);
        }
        if record.state == SubmissionState::Cancelled {
            return Err(AppError::Conflict("Download request was cancelled".into()));
        }
        options
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidInput("Download options must be an object".into()))?
            .insert("gid".into(), record.gid.into());
    }
    preferences::validate_save_location(app, &options)?;
    let automatic = if let TaskInput::Uris(uris) = &input {
        native::prepare(app, &engine, uris, &mut options).await?
    } else {
        None
    };
    let generation = engine.generation();
    let _mutation = engine.mutation.lock().await;
    let result = if let Some(id) = request_id {
        let gate = app.state::<SubmissionGate>();
        let _submission = gate.0.lock().await;
        submit_reserved(&engine, &app.state::<DatabaseState>().0, id, input, options).await
    } else {
        input.create(&engine, options).await
    };
    if let Ok(gid) = &result {
        super::tasks::notify_changed(app, gid);
    }
    drop(_mutation);
    native::finish(app.clone(), engine, automatic, generation).await;
    result
}

async fn submit_reserved(
    engine: &crate::services::tasks::TaskService,
    db: &crate::database::Database,
    id: &str,
    input: TaskInput,
    mut options: serde_json::Value,
) -> Result<String, AppError> {
    let record = db
        .submission(id)
        .await?
        .ok_or_else(|| AppError::NotFound("Download request expired".into()))?;
    if record.state == SubmissionState::Submitted {
        return Ok(record.gid);
    }
    if record.state == SubmissionState::Cancelled {
        return Err(AppError::Conflict("Download request was cancelled".into()));
    }
    let exists = engine
        .tell_task_snapshot(true)
        .await?
        .iter()
        .any(|task| task.gid == record.gid);
    if !exists {
        options
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidInput("Download options must be an object".into()))?
            .insert("gid".into(), record.gid.clone().into());
        input.create(engine, options).await?;
    }
    engine.save_session().await?;
    db.commit_submission(id).await?;
    Ok(record.gid)
}

pub async fn restore_pending(app: &AppHandle) -> Result<(), AppError> {
    for value in app.state::<DatabaseState>().0.pending_downloads().await? {
        let request: AddRequest = serde_json::from_str(&value)?;
        show_confirmation(app, &request);
    }
    Ok(())
}

fn show_confirmation(app: &AppHandle, request: &AddRequest) {
    let input = crate::services::external_input::ExternalDownloadInput {
        request_id: Some(request.id.clone()),
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

pub async fn cancel(app: &AppHandle, id: &str) -> Result<(), AppError> {
    let gate = app.state::<SubmissionGate>();
    let _submission = gate.0.lock().await;
    app.state::<DatabaseState>().0.cancel_submission(id).await
}

#[cfg(test)]
mod tests;
