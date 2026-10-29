//! `schedule` entries of default-branch workflows and their next fire times (migration
//! `0048_actions.sql`). A repository's entries are replaced as a whole whenever its default
//! branch moves; the ticker reads what is due through a partial index.
//!
//! The Actions foundation (workstream F) owns this file.

use rbit_core::{RepoId, Timestamp, actions::ScheduleEntry};
use sqlx::SqlitePool;

use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct ScheduleRow {
    repo_id: i64,
    workflow_path: String,
    cron: String,
    next_at: i64,
    failures: i64,
    paused: bool,
}

impl From<ScheduleRow> for ScheduleEntry {
    fn from(row: ScheduleRow) -> Self {
        Self {
            repo_id: RepoId(row.repo_id),
            workflow_path: row.workflow_path,
            cron: row.cron,
            next_at: Timestamp(row.next_at),
            failures: row.failures,
            paused: row.paused,
        }
    }
}

/// One entry of a repository's new schedule.
#[derive(Debug, Clone)]
pub struct NewSchedule<'a> {
    pub workflow_path: &'a str,
    pub cron: &'a str,
    pub next_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct ActionsScheduleStore {
    pool: SqlitePool,
}

impl ActionsScheduleStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Replaces the repository's schedule. Entries that stay keep their failure count and
    /// pause; new ones start fresh.
    pub async fn replace(&self, repo: RepoId, entries: &[NewSchedule<'_>]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let keep = serde_json::to_string(&entries.iter().map(|entry| [entry.workflow_path, entry.cron]).collect::<Vec<_>>())
            .map_err(|error| StorageError::Corrupt(error.to_string()))?;
        sqlx::query(
            "DELETE FROM actions_schedules WHERE repo_id = ?1 AND NOT EXISTS (SELECT 1 FROM json_each(?2) k \
             WHERE json_extract(k.value, '$[0]') = workflow_path AND json_extract(k.value, '$[1]') = cron)",
        )
        .bind(repo.0)
        .bind(keep)
        .execute(&mut *tx)
        .await?;
        for entry in entries {
            sqlx::query(
                "INSERT INTO actions_schedules (repo_id, workflow_path, cron, next_at) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (repo_id, workflow_path, cron) DO UPDATE SET next_at = ?4",
            )
            .bind(repo.0)
            .bind(entry.workflow_path)
            .bind(entry.cron)
            .bind(entry.next_at.0)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The repository's schedule entries.
    pub async fn list(&self, repo: RepoId) -> Result<Vec<ScheduleEntry>, StorageError> {
        let rows: Vec<ScheduleRow> = sqlx::query_as(
            "SELECT repo_id, workflow_path, cron, next_at, failures, paused FROM actions_schedules WHERE repo_id = ? \
             ORDER BY workflow_path, cron",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ScheduleEntry::from).collect())
    }

    /// Entries due at `now` that are not paused, earliest first.
    pub async fn due(&self, now: Timestamp, limit: u32) -> Result<Vec<ScheduleEntry>, StorageError> {
        let rows: Vec<ScheduleRow> = sqlx::query_as(
            "SELECT repo_id, workflow_path, cron, next_at, failures, paused FROM actions_schedules WHERE paused = 0 AND next_at <= ? \
             ORDER BY next_at LIMIT ?",
        )
        .bind(now.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ScheduleEntry::from).collect())
    }

    /// The earliest next fire time of any entry that is not paused (the ticker's sleep).
    pub async fn next_due(&self) -> Result<Option<Timestamp>, StorageError> {
        let next: Option<i64> =
            sqlx::query_scalar("SELECT MIN(next_at) FROM actions_schedules WHERE paused = 0").fetch_one(&self.pool).await?;
        Ok(next.map(Timestamp))
    }

    /// Records a tick: the next fire time, and whether the run planned (`failed` counts
    /// consecutive failures; `pause_after` of them pause the entry).
    pub async fn record_tick(
        &self,
        repo: RepoId,
        workflow_path: &str,
        cron: &str,
        next_at: Timestamp,
        failed: bool,
        pause_after: i64,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE actions_schedules SET next_at = ?1, failures = CASE WHEN ?2 THEN failures + 1 ELSE 0 END, \
             paused = CASE WHEN ?2 AND failures + 1 >= ?3 THEN 1 ELSE paused END \
             WHERE repo_id = ?4 AND workflow_path = ?5 AND cron = ?6",
        )
        .bind(next_at.0)
        .bind(failed)
        .bind(pause_after)
        .bind(repo.0)
        .bind(workflow_path)
        .bind(cron)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Pauses or resumes every entry of the repository (archived, Actions off).
    pub async fn set_paused(&self, repo: RepoId, paused: bool) -> Result<(), StorageError> {
        sqlx::query("UPDATE actions_schedules SET paused = ?, failures = CASE WHEN ? THEN failures ELSE 0 END WHERE repo_id = ?")
            .bind(paused)
            .bind(paused)
            .bind(repo.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
