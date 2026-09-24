//! Admission and visibility of browser probes and automatically selected tasks.
use crate::aria2::types::Aria2Task;
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::RwLock;

#[derive(Default)]
pub struct TaskPolicy {
    internal: RwLock<HashSet<String>>,
    automatic: RwLock<HashSet<String>>,
    pending_starts: RwLock<HashSet<String>>,
    known: RwLock<HashSet<String>>,
    worker: AtomicBool,
    deleted: std::sync::Mutex<HashSet<String>>,
}
impl TaskPolicy {
    pub fn mark_deleted(&self, gid: &str) {
        self.deleted
            .lock()
            .expect("task deletion state poisoned")
            .insert(gid.into());
    }
    pub fn is_deleted(&self, gid: &str) -> bool {
        self.deleted
            .lock()
            .expect("task deletion state poisoned")
            .contains(gid)
    }

    pub async fn has_internal(&self) -> bool {
        !self.internal.read().await.is_empty()
    }
    pub async fn internal_ids(&self) -> HashSet<String> {
        self.internal.read().await.clone()
    }
    pub async fn clear_automatic(&self) {
        self.automatic.write().await.clear();
    }
    pub async fn clear_pending_starts(&self) {
        self.deleted
            .lock()
            .expect("task deletion state poisoned")
            .clear();
        self.pending_starts.write().await.clear();
        self.known.write().await.clear();
    }
    pub async fn expect_start(&self, gid: &str) {
        self.known.write().await.insert(gid.into());
        self.pending_starts.write().await.insert(gid.into());
    }
    pub async fn remember_tasks(&self, gids: impl IntoIterator<Item = String>) {
        self.known.write().await.extend(gids);
    }
    /// A start for a GID the app has never seen came from another aria2 RPC
    /// client, such as a browser extension. It is queued like an in-app
    /// submission. Resumes of known tasks are left alone.
    pub async fn claim_external_start(&self, gid: &str) -> bool {
        if !self.known.write().await.insert(gid.into()) {
            return false;
        }
        self.pending_starts.write().await.insert(gid.into());
        true
    }
    pub async fn take_start(&self, gid: &str) -> bool {
        self.pending_starts.write().await.remove(gid)
    }
    pub fn begin_automatic_worker(&self) -> bool {
        !self.worker.swap(true, Ordering::AcqRel)
    }
    pub fn end_automatic_worker(&self) {
        self.worker.store(false, Ordering::Release);
    }
    pub async fn automatic_ids(&self) -> std::collections::HashSet<String> {
        self.automatic.read().await.clone()
    }
    pub async fn set_automatic(&self, gid: &str, enabled: bool) {
        let mut gids = self.automatic.write().await;
        if enabled {
            gids.insert(gid.into());
        } else {
            gids.remove(gid);
        }
    }
    pub async fn is_automatic(&self, gid: &str) -> bool {
        self.automatic.read().await.contains(gid)
    }
    pub async fn set_internal(&self, gid: &str, internal: bool) {
        let mut gids = self.internal.write().await;
        if internal {
            gids.insert(gid.to_string());
        } else {
            gids.remove(gid);
        }
    }
    pub async fn is_internal(&self, gid: &str) -> bool {
        self.internal.read().await.contains(gid)
    }
    pub async fn visible_tasks(&self, mut tasks: Vec<Aria2Task>) -> Vec<Aria2Task> {
        let gids = self.internal.read().await;
        tasks.retain(|task| !gids.contains(&task.gid));
        let automatic = self.automatic.read().await;
        for task in &mut tasks {
            task.selection_managed = automatic.contains(&task.gid);
        }
        tasks
    }
}

#[cfg(test)]
mod tests {
    use super::TaskPolicy;

    #[tokio::test]
    async fn an_unseen_start_is_queued_once() {
        let policy = TaskPolicy::default();
        assert!(policy.claim_external_start("external").await);
        assert!(!policy.claim_external_start("external").await);
        assert!(policy.take_start("external").await);
        assert!(!policy.claim_external_start("external").await);
        assert!(!policy.take_start("external").await);
    }

    #[tokio::test]
    async fn app_submissions_are_not_claimed_again_after_they_notify() {
        let policy = TaskPolicy::default();
        policy.expect_start("submitted").await;
        assert!(policy.take_start("submitted").await);
        assert!(!policy.claim_external_start("submitted").await);
        assert!(!policy.take_start("submitted").await);
    }

    #[tokio::test]
    async fn tasks_that_existed_before_the_listener_connected_stay_silent() {
        let policy = TaskPolicy::default();
        policy.remember_tasks(["restored".to_string()]).await;
        assert!(!policy.claim_external_start("restored").await);
        assert!(!policy.take_start("restored").await);
    }

    #[tokio::test]
    async fn an_engine_restart_forgets_known_tasks() {
        let policy = TaskPolicy::default();
        policy.remember_tasks(["old".to_string()]).await;
        policy.clear_pending_starts().await;
        assert!(policy.claim_external_start("old").await);
    }
}
