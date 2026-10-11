//! Media operations: one hidden engine task from probe to submission.
//!
//! Operations live in memory. The engine task is the durable part: when the
//! desktop restarts mid-selection, the paused task simply appears in the task
//! list and the extension's next request reports the operation as unknown.
use super::contracts::{Format, Presentation, Selection, RECEIPT_MS};
use super::error::Error;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Probing,
    Ready,
    Starting,
    Submitted,
    Cancelled,
    Failed,
}
impl State {
    pub fn pending(self) -> bool {
        matches!(self, Self::Probing | Self::Ready)
    }
    pub fn active(self) -> bool {
        self.pending() || self == Self::Starting
    }
}

#[derive(Clone)]
pub struct Operation {
    pub id: Uuid,
    pub fingerprint: String,
    pub expires_at: i64,
    pub retain_until: i64,
    pub gid: String,
    pub state: State,
    pub format: Format,
    pub presentation: Option<Presentation>,
    pub selection: Option<Selection>,
    pub submission_id: Option<Uuid>,
    pub error: Option<Error>,
    pub capture_ids: Vec<Uuid>,
}
impl Operation {
    pub fn new(id: Uuid, fingerprint: String, now: i64, format: Format) -> Self {
        Self {
            id,
            fingerprint,
            expires_at: now + super::contracts::LEASE_MS,
            retain_until: now + RECEIPT_MS,
            gid: new_gid(),
            state: State::Probing,
            format,
            presentation: None,
            selection: None,
            submission_id: None,
            error: None,
            capture_ids: vec![],
        }
    }
    pub fn response(&self) -> serde_json::Value {
        let mut value = serde_json::json!({"id":self.id,"expiresAt":self.expires_at,
            "state":self.state});
        if self.state == State::Starting {
            value["state"] = "submitting".into();
            value["submissionId"] = serde_json::json!(self.submission_id);
        }
        match self.state {
            State::Ready => value["presentation"] = serde_json::json!(self.presentation),
            State::Submitted => {
                value["submissionId"] = serde_json::json!(self.submission_id);
                value["gid"] = self.gid.clone().into();
            }
            State::Failed => {
                value["error"] = serde_json::json!(self.error.unwrap_or(Error::ProbeFailed))
            }
            _ => {}
        }
        value
    }
}
fn new_gid() -> String {
    Uuid::new_v4().simple().to_string()[..16].to_string()
}
