//! The desktop owns the ordinary download handoff contract.
//!
//! A browser download stays held in the browser until `/add` answers. The
//! desktop answers `added` once the engine owns the download, `cancelled`
//! when the user declined it, and `fallback` when the browser should keep it.
//! A lost answer therefore never loses a download: the extension lets the
//! browser continue.
//!
//! Chromium waits at most 15 s for an extension to name a download, so a
//! request with `ack: "early"` is answered `accepted` as soon as the add
//! dialog shows it: from then on the dialog owns the download, as in other
//! download managers, and the browser drops its copy.
use crate::services::external_input::ExternalRequestHeader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub url: String,
    pub final_url: Option<String>,
    pub referer: Option<String>,
    pub cookie: Option<String>,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub request_headers: Vec<ExternalRequestHeader>,
    pub filename: Option<String>,
    #[serde(default)]
    pub filename_source: FilenameSource,
    /// The browser holds a download it started; check that the engine
    /// receives the same file before taking it over.
    #[serde(default)]
    pub verify: bool,
    /// MIME type and size the browser received, when known.
    pub mime: Option<String>,
    pub size: Option<i64>,
    /// When the extension wants the answer; older extensions omit it.
    #[serde(default)]
    pub ack: Ack,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ack {
    /// Once the user decided in the add dialog.
    #[default]
    Final,
    /// As soon as the add dialog shows the download.
    Early,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FilenameSource {
    Browser,
    #[default]
    Suggested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Added,
    /// The add dialog took the download over; the browser drops its copy.
    Accepted,
    Cancelled,
    Fallback,
}

/// Why the browser keeps a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FallbackReason {
    /// No response: DNS, connection or TLS failure, or a timeout.
    Unreachable,
    /// The server refused the engine's request (4xx/5xx).
    Rejected,
    /// The engine got a web page where the browser got a file, typically a
    /// login or consent page because a session is missing.
    WebPage,
    /// The engine would receive a different file.
    SizeMismatch,
    /// The confirmation dialog was not answered in time.
    Unconfirmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddResponse {
    pub action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<FallbackReason>,
}

impl AddResponse {
    pub fn added(gid: String) -> Self {
        Self {
            action: Action::Added,
            gid: Some(gid),
            reason: None,
        }
    }
    pub fn accepted() -> Self {
        Self {
            action: Action::Accepted,
            gid: None,
            reason: None,
        }
    }
    pub fn cancelled() -> Self {
        Self {
            action: Action::Cancelled,
            gid: None,
            reason: None,
        }
    }
    pub fn fallback(reason: FallbackReason) -> Self {
        Self {
            action: Action::Fallback,
            gid: None,
            reason: Some(reason),
        }
    }
}
