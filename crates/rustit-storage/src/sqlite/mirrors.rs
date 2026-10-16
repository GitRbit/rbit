//! Pull mirror settings and fetch status.
//!
//! Milestone 2 workstream R (repository essentials) owns this file. Keep row types here rather than in `rows.rs`.

use rustit_core::{RepoId, Timestamp, mirror::Mirror};
use sqlx::SqlitePool;

use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct MirrorRow {
    repo_id: i64,
    url: String,
    interval_minutes: i64,
    next_sync_at: i64,
    last_sync_at: Option<i64>,
    last_error: Option<String>,
    created_at: i64,
}

impl TryFrom<MirrorRow> for Mirror {
    type Error = StorageError;

    fn try_from(row: MirrorRow) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_id: RepoId(row.repo_id),
            url: row.url.parse().map_err(|e| StorageError::Corrupt(format!("mirror URL: {e}")))?,
            interval_minutes: u64::try_from(row.interval_minutes).map_err(|_| StorageError::Corrupt("mirror interval".to_owned()))?,
            next_sync_at: Timestamp(row.next_sync_at),
            last_sync_at: row.last_sync_at.map(Timestamp),
            last_error: row.last_error,
            created_at: Timestamp(row.created_at),
        })
    }
}

/// Mirror columns in [`MirrorRow`] order.
macro_rules! select_mirror {
    ($clause:expr) => {
        concat!("SELECT repo_id, url, interval_minutes, next_sync_at, last_sync_at, last_error, created_at FROM repo_mirrors ", $clause)
    };
}

fn minutes(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::Corrupt("mirror interval out of range".to_owned()))
}

#[derive(Debug, Clone)]
pub struct MirrorStore {
    pool: SqlitePool,
}

impl MirrorStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Makes `repo` a mirror of `url`, due for its first fetch immediately.
    pub async fn create(&self, repo: RepoId, url: &str, interval_minutes: u64) -> Result<(), StorageError> {
        let now = Timestamp::now().seconds();
        sqlx::query("INSERT INTO repo_mirrors (repo_id, url, interval_minutes, next_sync_at, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(repo.0)
            .bind(url)
            .bind(minutes(interval_minutes)?)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::from_insert(e, "mirror"))?;
        Ok(())
    }

    pub async fn find(&self, repo: RepoId) -> Result<Option<Mirror>, StorageError> {
        let row: Option<MirrorRow> = sqlx::query_as(select_mirror!("WHERE repo_id = ?")).bind(repo.0).fetch_optional(&self.pool).await?;
        row.map(Mirror::try_from).transpose()
    }

    pub async fn is_mirror(&self, repo: RepoId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT 1 FROM repo_mirrors WHERE repo_id = ?")
            .bind(repo.0)
            .fetch_optional(&self.pool)
            .await?
            .is_some())
    }

    /// Changes the interval; the next fetch is rescheduled from the last one.
    pub async fn set_interval(&self, repo: RepoId, interval_minutes: u64) -> Result<(), StorageError> {
        let interval = minutes(interval_minutes)?;
        sqlx::query(
            "UPDATE repo_mirrors SET interval_minutes = ?1, \
             next_sync_at = COALESCE(last_sync_at, created_at) + ?1 * 60 WHERE repo_id = ?2",
        )
        .bind(interval)
        .bind(repo.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Makes the mirror due now.
    pub async fn schedule_now(&self, repo: RepoId) -> Result<(), StorageError> {
        sqlx::query("UPDATE repo_mirrors SET next_sync_at = ? WHERE repo_id = ?")
            .bind(Timestamp::now().seconds())
            .bind(repo.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Stops mirroring; the repository stays.
    pub async fn delete(&self, repo: RepoId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM repo_mirrors WHERE repo_id = ?").bind(repo.0).execute(&self.pool).await?;
        Ok(())
    }

    /// Mirrors whose next fetch is due at `now`, longest overdue first.
    pub async fn due(&self, now: Timestamp, limit: u32) -> Result<Vec<Mirror>, StorageError> {
        let rows: Vec<MirrorRow> = sqlx::query_as(select_mirror!("WHERE next_sync_at <= ? ORDER BY next_sync_at, repo_id LIMIT ?"))
            .bind(now.seconds())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Mirror::try_from).collect()
    }

    /// Records a fetch at `at` (with its error, if it failed) and schedules the next one
    /// an interval later.
    pub async fn record_sync(&self, repo: RepoId, at: Timestamp, error: Option<&str>) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE repo_mirrors SET last_sync_at = ?1, last_error = ?2, next_sync_at = ?1 + interval_minutes * 60 WHERE repo_id = ?3",
        )
        .bind(at.seconds())
        .bind(error)
        .bind(repo.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
