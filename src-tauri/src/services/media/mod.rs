//! Native HLS/DASH inspection leases and extension submission receipts.
mod assets;
pub mod contracts;
pub mod error;
mod operation;
mod probe;
mod runtime;
pub use runtime::{owns, service, MediaState};
pub mod native;
mod policy;
pub mod routes;
#[cfg(test)]
mod tests;
pub use policy::start_automatic_selection;

use crate::services::tasks::TaskService;
use contracts::*;
use error::Error;
use operation::{Operation, State};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct MediaService {
    engine: Arc<TaskService>,
    submission_gate: Mutex<()>,
    deferred_events: Mutex<HashMap<String, (&'static str, crate::aria2::types::Aria2Task)>>,
    operations: Mutex<HashMap<Uuid, Operation>>,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
impl MediaService {
    fn new(engine: Arc<TaskService>) -> Self {
        Self {
            engine,
            submission_gate: Mutex::new(()),
            deferred_events: Mutex::new(HashMap::new()),
            operations: Mutex::new(HashMap::new()),
        }
    }

    pub async fn create(
        self: &Arc<Self>,
        request: ProbeRequest,
        format: Format,
    ) -> Result<Value, Error> {
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&request).map_err(|_| Error::UnsupportedSource)?)
        );
        let mut records = self.operations.lock().await;
        if let Some(existing) = records.get(&request.id) {
            if existing.state == State::Cancelled || existing.fingerprint == fingerprint {
                if now() >= existing.expires_at && existing.state.pending() {
                    return Err(Error::Expired);
                }
                return Ok(existing.response());
            }
            return Err(Error::Conflict);
        }
        request.source.validate()?;
        if records.values().filter(|op| op.state.active()).count() >= 8 || records.len() >= 4096 {
            return Err(Error::Unavailable);
        }
        let mut record = Operation::new(request.id, fingerprint, now(), format);
        record.capture_ids = request
            .source
            .input
            .tracks
            .iter()
            .flat_map(|track| track.urls.iter())
            .filter_map(|url| assets::capture_id(url))
            .collect();
        record.capture_ids.sort();
        record.capture_ids.dedup();
        self.engine.tasks.set_internal(&record.gid, true).await;
        records.insert(record.id, record.clone());
        let response = record.response();
        let worker = self.clone();
        tokio::spawn(async move {
            worker.probe(record, request.source).await;
        });
        Ok(response)
    }

    pub async fn read(&self, id: Uuid) -> Result<Value, Error> {
        let records = self.operations.lock().await;
        let record = records.get(&id).ok_or(Error::NotFound)?;
        if now() >= record.expires_at && record.state.pending() {
            return Err(Error::Expired);
        }
        Ok(record.response())
    }

    pub async fn submit(
        self: &Arc<Self>,
        id: Uuid,
        request: SubmitRequest,
    ) -> Result<Value, Error> {
        let mut records = self.operations.lock().await;
        let record = records.get_mut(&id).ok_or(Error::NotFound)?;
        if let Some(previous) = record.submission_id {
            if previous != request.submission_id
                || record.selection.as_ref() != Some(&request.selection)
            {
                return Err(Error::Conflict);
            }
            return match record.state {
                State::Submitted => Ok(receipt(record)),
                State::Failed => Err(Error::SourceExpired),
                _ => Err(Error::Unavailable),
            };
        }
        if now() >= record.expires_at {
            return Err(Error::Expired);
        }
        if record.state != State::Ready {
            return Err(Error::Conflict);
        }
        record
            .presentation
            .as_ref()
            .ok_or(Error::Conflict)?
            .validate_selection(&request.selection)?;
        let mut intent = record.clone();
        intent.selection = Some(request.selection);
        intent.submission_id = Some(request.submission_id);
        intent.state = State::Starting;
        intent.retain_until = now() + RECEIPT_MS;
        *record = intent.clone();
        drop(records);
        let worker = self.clone();
        let job = tokio::spawn(async move { worker.start(intent).await });
        match tokio::time::timeout(Duration::from_secs(4), job).await {
            Ok(Ok(result)) => result,
            _ => Err(Error::Unavailable),
        }
    }

    async fn start(&self, record: Operation) -> Result<Value, Error> {
        let _submission = self.submission_gate.lock().await;
        if let Some(current) = self.operations.lock().await.get(&record.id) {
            if current.state == State::Submitted {
                return Ok(receipt(current));
            }
        }
        self.start_native(&record).await?;
        let mut records = self.operations.lock().await;
        let current = records.get_mut(&record.id).ok_or(Error::NotFound)?;
        let mut completed = current.clone();
        completed.state = State::Submitted;
        completed.error = None;
        *current = completed;
        self.engine.tasks.set_internal(&record.gid, false).await;
        Ok(receipt(current))
    }

    async fn start_native(&self, record: &Operation) -> Result<(), Error> {
        let task = self
            .engine
            .hidden_tasks()
            .await
            .map_err(|_| Error::Unavailable)?
            .into_iter()
            .find(|task| task.gid == record.gid)
            .ok_or(Error::SourceExpired)?;
        let selection = record.selection.as_ref().ok_or(Error::Conflict)?;
        native::start(
            &self.engine,
            &task,
            selection.options(),
            native::StartMode::Browser,
        )
        .await
        .map_err(|_| Error::Unavailable)?;
        Ok(())
    }

    pub async fn cancel(&self, id: Uuid) -> Result<Value, Error> {
        let mut records = self.operations.lock().await;
        if !records.contains_key(&id) && records.len() >= 4096 {
            return Err(Error::Unavailable);
        }
        let existing = records
            .get(&id)
            .cloned()
            .unwrap_or_else(|| Operation::new(id, String::new(), now(), Format::Mp4));
        if existing.state == State::Submitted {
            let mut value = receipt(&existing);
            value["state"] = "submitted".into();
            return Ok(value);
        }
        if existing.state == State::Starting {
            return Err(Error::Unavailable);
        }
        let mut cancelled = existing;
        cancelled.state = State::Cancelled;
        records.insert(id, cancelled);
        Ok(json!({"id":id,"state":"cancelled"}))
    }

    async fn clean_gid(&self, gid: &str) -> Result<(), Error> {
        let tasks = self
            .engine
            .hidden_tasks()
            .await
            .map_err(|_| Error::Unavailable)?;
        let Some(task) = tasks.into_iter().find(|task| task.gid == gid) else {
            return Ok(());
        };
        if !matches!(task.status.as_str(), "complete" | "error" | "removed") {
            self.engine
                .force_remove(gid)
                .await
                .map_err(|_| Error::Unavailable)?;
        }
        self.engine
            .remove_download_result(gid)
            .await
            .map_err(|_| Error::Unavailable)?;
        Ok(())
    }

    async fn maintain(&self) -> Result<(), Error> {
        let records: Vec<_> = self.operations.lock().await.values().cloned().collect();
        let present: std::collections::HashSet<_> = self
            .engine
            .hidden_tasks()
            .await
            .map_err(|_| Error::Unavailable)?
            .into_iter()
            .map(|task| task.gid)
            .collect();
        for record in records {
            if record.state == State::Starting {
                match self.start(record.clone()).await {
                    Err(Error::SourceExpired) => {
                        let mut all = self.operations.lock().await;
                        if let Some(current) = all.get_mut(&record.id) {
                            let mut failed = current.clone();
                            failed.state = State::Failed;
                            failed.error = Some(Error::SourceExpired);
                            *current = failed;
                        }
                    }
                    Err(code) => {
                        log::debug!("media: submission reconciliation pending code={code}")
                    }
                    Ok(_) => {}
                }
                continue;
            }
            if record.expires_at <= now() && record.state.pending() {
                self.cancel(record.id).await?;
            }
            if matches!(record.state, State::Failed | State::Cancelled)
                && present.contains(&record.gid)
            {
                self.clean_gid(&record.gid).await?;
            }
            if record.retain_until <= now() && !record.state.pending() {
                self.deferred_events.lock().await.remove(&record.gid);
                self.engine.tasks.set_internal(&record.gid, false).await;
                self.operations.lock().await.remove(&record.id);
            }
        }
        Ok(())
    }
}
fn receipt(record: &Operation) -> Value {
    json!({"id":record.id,"submissionId":record.submission_id,"gid":record.gid})
}
