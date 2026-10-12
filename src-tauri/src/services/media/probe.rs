//! Engine capability negotiation and native manifest inspection.
use super::{
    contracts::*,
    error::Error,
    now,
    operation::{Operation, State},
    MediaService,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

fn native_error(task: &crate::aria2::types::Aria2Task) -> Error {
    match task.media.as_ref().map(|media| media.error_code.as_str()) {
        Some("unsupported_source") => Error::UnsupportedSource,
        Some("protected_media") => Error::ProtectedMedia,
        Some("authentication_required") => Error::AuthenticationRequired,
        Some("unsupported_selection") => Error::UnsupportedSelection,
        _ => Error::ProbeFailed,
    }
}

impl MediaService {
    pub async fn capabilities(&self) -> Result<Value, Error> {
        let (version, methods) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::try_join!(self.engine.get_version(), self.engine.list_methods())
        })
        .await
        .map_err(|_| Error::Unavailable)?
        .map_err(|_| Error::Unavailable)?;
        let media_features = version["mediaFeatures"]
            .as_array()
            .ok_or(Error::Unavailable)?;
        if ![
            "request-contexts",
            "stable-track-ids",
            "structured-errors",
            "captured-inputs",
        ]
        .iter()
        .all(|required| media_features.iter().any(|feature| feature == required))
        {
            return Err(Error::IntegrationUnavailable);
        }
        let features = version["enabledFeatures"]
            .as_array()
            .ok_or(Error::Unavailable)?;
        if !features.iter().any(|f| f == "HLS/DASH")
            || !["aria2.finishMedia", "aria2.retryMedia"]
                .iter()
                .all(|name| methods.iter().any(|method| method == name))
        {
            return Err(Error::IntegrationUnavailable);
        }
        Ok(
            json!({"product":"rayburst","protocolVersion":2,"sourceKinds":["hls","dash","collection"],"requestContexts":true}),
        )
    }

    pub(super) async fn probe(self: Arc<Self>, record: Operation, source: Source) {
        let result = self.run_probe(&record, &source).await;
        let mut records = self.operations.lock().await;
        let Some(current) = records.get_mut(&record.id) else {
            return;
        };
        if current.state != State::Probing {
            drop(records);
            let _ = self.clean_gid(&record.gid).await;
            return;
        }
        let mut updated = current.clone();
        match result {
            Ok(presentation) => {
                updated.presentation = Some(presentation);
                updated.state = State::Ready;
            }
            Err(code) => {
                updated.state = State::Failed;
                updated.error = Some(code);
            }
        }
        *current = updated;
    }

    async fn run_probe(&self, record: &Operation, source: &Source) -> Result<Presentation, Error> {
        let mut options = json!({"gid":record.gid,"media":source.kind,"media-format":record.format,
            "media-pause-after-probe":"true","header":"","http-user":"","http-passwd":"","referer":source.page_referrer(),
            "check-certificate":"true","auto-file-renaming":"true","load-cookies":"",
            "media-request-contexts": serde_json::to_string(&source.request_contexts.iter()
                .map(|context| json!({"url":context.url,"headers":context.headers}))
                .collect::<Vec<_>>()).map_err(|_| Error::UnsupportedSource)?});
        options["media-input"] = serde_json::to_string(&source.input)
            .map_err(|_| Error::UnsupportedSource)?
            .into();
        if !source.input.tracks.is_empty()
            && source
                .input
                .tracks
                .iter()
                .any(|track| track.r#type == "subtitle")
        {
            options["media-subtitles"] = "best".into();
        }
        options["filename-hint"] = source.filename_hint().into();
        options["filename-hint-source"] = if source.title.trim().is_empty() {
            "suggested"
        } else {
            "title"
        }
        .into();
        self.engine
            .add_uri(vec![source.url.clone()], options)
            .await
            .map_err(|_| Error::ProbeFailed)?;
        loop {
            if now() >= record.expires_at {
                return Err(Error::Expired);
            }
            if self
                .operations
                .lock()
                .await
                .get(&record.id)
                .is_none_or(|op| op.state != State::Probing)
            {
                return Err(Error::Expired);
            }
            let task = self
                .engine
                .tell_status(&record.gid)
                .await
                .map_err(|_| Error::ProbeFailed)?;
            if task.status == "error" {
                return Err(native_error(&task));
            }
            if task.status == "paused"
                && task
                    .media
                    .as_ref()
                    .is_some_and(|m| m.state == "awaiting-selection")
            {
                return Presentation::from_task(&task, record.format);
            }
            if matches!(task.status.as_str(), "complete" | "removed") {
                return Err(Error::UnsupportedSource);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}
