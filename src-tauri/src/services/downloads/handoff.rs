//! Browser downloads held open while the add dialog decides.
//!
//! Nothing is persisted: the browser keeps holding its download until the
//! HTTP answer, so a desktop that exits mid-decision simply leaves the
//! download to the browser.
use crate::error::AppError;
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};
use tokio::sync::oneshot;

/// Tells the add dialog that a held download went back to the browser.
pub const EXPIRED_EVENT: &str = "downloads:handoff-expired";

/// Called with the request ID when a held download leaves the dialog.
pub(super) type OnWithdraw = Box<dyn Fn(&str) + Send + Sync>;

pub enum Outcome {
    Added(String),
    Cancelled,
}

struct Entry {
    reply: oneshot::Sender<Outcome>,
    /// A task is being created for it; only that result answers now.
    claimed: bool,
}

enum Withdrawal {
    Withdrawn,
    Claimed,
    Answered,
}

#[derive(Default)]
pub struct Handoffs(Mutex<HashMap<String, Entry>>);

impl Handoffs {
    fn entries(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn open(&self, on_withdraw: OnWithdraw) -> Pending<'_> {
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, answer) = oneshot::channel();
        self.entries().insert(
            id.clone(),
            Entry {
                reply,
                claimed: false,
            },
        );
        Pending {
            handoffs: self,
            on_withdraw,
            id,
            answer,
        }
    }

    /// Reserves a held download for the task the dialog is creating.
    pub(super) fn claim(&self, id: &str) -> Result<Claim<'_>, AppError> {
        match self.entries().get_mut(id) {
            Some(entry) if !entry.claimed => {
                entry.claimed = true;
                Ok(Claim {
                    handoffs: self,
                    id: id.to_owned(),
                })
            }
            Some(_) => Err(AppError::Conflict(
                "This download is already being added".into(),
            )),
            None => Err(AppError::NotFound(
                "The browser has taken this download back".into(),
            )),
        }
    }

    pub(super) fn cancel(&self, id: &str) {
        let mut entries = self.entries();
        if entries.get(id).is_some_and(|entry| !entry.claimed) {
            if let Some(entry) = entries.remove(id) {
                let _ = entry.reply.send(Outcome::Cancelled);
            }
        }
    }

    fn withdraw(&self, id: &str, on_withdraw: &OnWithdraw) -> Withdrawal {
        let mut entries = self.entries();
        match entries.get(id) {
            None => Withdrawal::Answered,
            Some(entry) if entry.claimed => Withdrawal::Claimed,
            Some(_) => {
                entries.remove(id);
                drop(entries);
                on_withdraw(id);
                Withdrawal::Withdrawn
            }
        }
    }
}

/// A task is being created for a held download.
pub(super) struct Claim<'a> {
    handoffs: &'a Handoffs,
    id: String,
}

impl Claim<'_> {
    pub(super) fn finish(self, gid: String) {
        if let Some(entry) = self.handoffs.entries().remove(&self.id) {
            let _ = entry.reply.send(Outcome::Added(gid));
        }
    }
}

impl Drop for Claim<'_> {
    /// A failed creation leaves the download to the dialog again.
    fn drop(&mut self) {
        if let Some(entry) = self.handoffs.entries().get_mut(&self.id) {
            entry.claimed = false;
        }
    }
}

/// A held download waiting for the dialog. Dropping it, for example when the
/// extension disconnects, withdraws the download from the dialog.
pub(super) struct Pending<'a> {
    handoffs: &'a Handoffs,
    on_withdraw: OnWithdraw,
    id: String,
    answer: oneshot::Receiver<Outcome>,
}

impl Pending<'_> {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    /// The dialog's answer, or `None` when it did not answer in time.
    pub(super) async fn wait(mut self, timeout: Duration) -> Option<Outcome> {
        if let Ok(answer) = tokio::time::timeout(timeout, &mut self.answer).await {
            return answer.ok();
        }
        loop {
            match self.handoffs.withdraw(&self.id, &self.on_withdraw) {
                Withdrawal::Withdrawn => return None,
                Withdrawal::Answered => return (&mut self.answer).await.ok(),
                // The task being created decides; retry if its creation fails.
                Withdrawal::Claimed => {
                    if let Ok(answer) =
                        tokio::time::timeout(Duration::from_secs(1), &mut self.answer).await
                    {
                        return answer.ok();
                    }
                }
            }
        }
    }
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.handoffs.withdraw(&self.id, &self.on_withdraw);
    }
}
