//! Superseded runs (migration `0076_actions_supersede.sql`): whether a repository lets a
//! newer push cancel a branch's older runs, and which commit superseded a cancelled run.
use std::collections::BTreeMap;

use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::{
    RepoId, Timestamp,
    actions::{RunId, TaskId, Termination, TerminationReason},
};

impl ActionsRunStore {
    /// Whether newer pushes to `repo`'s branches cancel the branches' older runs.
    pub async fn supersede_enabled(&self, repo: RepoId) -> Result<bool, StorageError> {
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT enabled FROM actions_supersede_settings WHERE repo_id = ?").bind(repo.0).fetch_optional(&self.pool).await?;
        Ok(enabled.unwrap_or(false))
    }

    pub async fn set_supersede(&self, repo: RepoId, enabled: bool) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_supersede_settings (repo_id, enabled) VALUES (?1, ?2) \
             ON CONFLICT (repo_id) DO UPDATE SET enabled = excluded.enabled",
        )
        .bind(repo.0)
        .bind(enabled)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Records that run `run` was superseded by commit `newer`, and that each of `tasks` (its
    /// tasks that were running) ended as `superseded`. The first record stands.
    pub async fn record_supersession(&self, run: RunId, newer: &str, tasks: &[TaskId], now: Timestamp) -> Result<(), StorageError> {
        sqlx::query("INSERT OR IGNORE INTO actions_supersessions (run_id, newer_sha, recorded_at) VALUES (?, ?, ?)")
            .bind(run.0)
            .bind(newer)
            .bind(now.0)
            .execute(&self.pool)
            .await?;
        for task in tasks {
            self.record_termination(*task, Termination::because(TerminationReason::Superseded), now).await?;
        }
        Ok(())
    }

    /// The commit that superseded each of `runs` that was superseded.
    pub async fn superseded_by(&self, runs: &[RunId]) -> Result<BTreeMap<RunId, String>, StorageError> {
        if runs.is_empty() {
            return Ok(BTreeMap::new());
        }
        let ids = format!("[{}]", runs.iter().map(|run| run.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT run_id, newer_sha FROM actions_supersessions WHERE run_id IN (SELECT value FROM json_each(?))")
                .bind(ids)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(run, sha)| (RunId(run), sha)).collect())
    }
}
