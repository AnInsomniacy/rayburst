//! Application selection intent; transfer state remains owned by the engine.
use super::Database;
use crate::error::AppError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MagnetSelectionPolicy {
    #[default]
    Prompt,
    Manual,
    DownloadAll,
}

#[derive(Clone, Copy, Debug)]
pub struct SelectionIntent {
    pub policy: MagnetSelectionPolicy,
    pub deferred: bool,
}

impl Database {
    pub async fn register_bt_selection(
        &self,
        gid: &str,
        policy: MagnetSelectionPolicy,
    ) -> Result<(), AppError> {
        self.connection().await?.execute(
            "INSERT OR IGNORE INTO bt_selection(gid, policy) VALUES (?1, ?2)",
            params![gid, serde_json::to_string(&policy)?],
        )?;
        Ok(())
    }

    pub async fn bt_selections(&self) -> Result<HashMap<String, SelectionIntent>, AppError> {
        let conn = self.connection().await?;
        let rows = conn
            .prepare("SELECT gid, policy, deferred FROM bt_selection")?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(gid, policy, deferred)| {
                Ok((
                    gid,
                    SelectionIntent {
                        policy: serde_json::from_str(&policy)?,
                        deferred,
                    },
                ))
            })
            .collect()
    }

    pub async fn defer_bt_selection(&self, gid: &str, deferred: bool) -> Result<(), AppError> {
        self.connection().await?.execute(
            "UPDATE bt_selection SET deferred=?2 WHERE gid=?1",
            params![gid, deferred],
        )?;
        Ok(())
    }

    pub async fn remove_bt_selection(&self, gid: &str) -> Result<(), AppError> {
        self.connection()
            .await?
            .execute("DELETE FROM bt_selection WHERE gid=?1", [gid])?;
        Ok(())
    }

    pub async fn defer_all_bt_selections(&self) -> Result<(), AppError> {
        self.connection()
            .await?
            .execute("UPDATE bt_selection SET deferred=1", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn selection_intent_survives_reopening_and_replayed_registration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.db");
        let db = Database::open(&path).unwrap();
        db.register_bt_selection("magnet", MagnetSelectionPolicy::Prompt)
            .await
            .unwrap();
        db.defer_bt_selection("magnet", true).await.unwrap();
        db.close().await;
        let db = Database::open(&path).unwrap();
        db.register_bt_selection("magnet", MagnetSelectionPolicy::DownloadAll)
            .await
            .unwrap();
        let intent = db.bt_selections().await.unwrap()["magnet"];
        assert_eq!(intent.policy, MagnetSelectionPolicy::Prompt);
        assert!(intent.deferred);
        db.remove_task_records("magnet").await.unwrap();
        assert!(db.bt_selections().await.unwrap().is_empty());
    }
}
