//! Push mirrors: remotes, their write-only credentials and the outcome of the last push
//! (migration `0039_push_mirrors.sql`).
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file. Keep row types
//! here rather than in `rows.rs`.
//!
//! The secret is read only by [`PushMirrorStore::target`], for the push itself; every
//! other query leaves it out and reports only whether one is set. It is stored
//! [sealed](crate::sealing).

use super::pool::Pool;
use rbit_core::{
    AccountId, RepoId, Timestamp,
    push_mirror::{PushMirror, PushMirrorId},
};

use crate::{StorageError, sealing::SealingKey};

#[derive(Debug, sqlx::FromRow)]
struct PushMirrorRow {
    id: i64,
    repo_id: i64,
    url: String,
    username: String,
    has_secret: bool,
    enabled: bool,
    sync_on_push: bool,
    next_sync_at: i64,
    last_attempt_at: Option<i64>,
    last_success_at: Option<i64>,
    last_error: Option<String>,
    created_by: Option<i64>,
    created_at: i64,
}

impl From<PushMirrorRow> for PushMirror {
    fn from(row: PushMirrorRow) -> Self {
        Self {
            id: PushMirrorId(row.id),
            repo_id: RepoId(row.repo_id),
            url: row.url,
            username: row.username,
            has_secret: row.has_secret,
            enabled: row.enabled,
            sync_on_push: row.sync_on_push,
            next_sync_at: Timestamp(row.next_sync_at),
            last_attempt_at: row.last_attempt_at.map(Timestamp),
            last_success_at: row.last_success_at.map(Timestamp),
            last_error: row.last_error,
            created_by: row.created_by.map(AccountId),
            created_at: Timestamp(row.created_at),
        }
    }
}

/// Push mirror columns in [`PushMirrorRow`] order, without the secret itself.
macro_rules! select_push_mirror {
    ($clause:expr) => {
        concat!(
            "SELECT id, repo_id, url, username, secret IS NOT NULL AS has_secret, enabled, sync_on_push, next_sync_at, \
             last_attempt_at, last_success_at, last_error, created_by, created_at FROM repo_push_mirrors ",
            $clause
        )
    };
}

/// A push mirror to create.
#[derive(Debug, Clone)]
pub struct NewPushMirror<'a> {
    pub repo: RepoId,
    pub url: &'a str,
    pub username: &'a str,
    pub secret: Option<&'a str>,
    pub sync_on_push: bool,
    pub created_by: AccountId,
}

/// Changed settings. `secret: None` keeps the stored secret.
#[derive(Debug, Clone)]
pub struct PushMirrorChanges<'a> {
    pub url: &'a str,
    pub username: &'a str,
    pub secret: Option<&'a str>,
    pub enabled: bool,
    pub sync_on_push: bool,
}

/// What a push needs: the mirror and its secret.
#[derive(Debug, Clone)]
pub struct PushTarget {
    pub mirror: PushMirror,
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PushMirrorStore {
    pool: Pool,
    key: SealingKey,
}

impl PushMirrorStore {
    pub(crate) fn new(pool: Pool, key: SealingKey) -> Self {
        Self { pool, key }
    }

    fn seal(&self, secret: Option<&str>) -> Result<Option<String>, StorageError> {
        Ok(secret.map(|secret| self.key.seal(secret.as_bytes())).transpose()?)
    }

    /// Adds a mirror, due for its first push immediately. [`StorageError::Conflict`] when
    /// the repository already pushes to `url`.
    pub async fn create(&self, new: &NewPushMirror<'_>) -> Result<PushMirror, StorageError> {
        let now = Timestamp::now().seconds();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO repo_push_mirrors (repo_id, url, username, secret, sync_on_push, next_sync_at, created_by, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6, ?6) RETURNING id",
        )
        .bind(new.repo.0)
        .bind(new.url)
        .bind(new.username)
        .bind(self.seal(new.secret)?)
        .bind(new.sync_on_push)
        .bind(now)
        .bind(new.created_by.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "a push mirror to this URL"))?;
        self.find(new.repo, PushMirrorId(id)).await?.ok_or_else(|| StorageError::Corrupt("push mirror vanished".to_owned()))
    }

    /// The mirrors of `repo`, oldest first.
    pub async fn list(&self, repo: RepoId) -> Result<Vec<PushMirror>, StorageError> {
        let rows: Vec<PushMirrorRow> =
            sqlx::query_as(select_push_mirror!("WHERE repo_id = ? ORDER BY id")).bind(repo.0).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(PushMirror::from).collect())
    }

    pub async fn count(&self, repo: RepoId) -> Result<u64, StorageError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM repo_push_mirrors WHERE repo_id = ?").bind(repo.0).fetch_one(&self.pool).await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// Mirror `id`, only if it belongs to `repo`.
    pub async fn find(&self, repo: RepoId, id: PushMirrorId) -> Result<Option<PushMirror>, StorageError> {
        let row: Option<PushMirrorRow> =
            sqlx::query_as(select_push_mirror!("WHERE id = ? AND repo_id = ?")).bind(id.0).bind(repo.0).fetch_optional(&self.pool).await?;
        Ok(row.map(PushMirror::from))
    }

    /// Mirror `id` with its secret, for pushing.
    pub async fn target(&self, id: PushMirrorId) -> Result<Option<PushTarget>, StorageError> {
        let row: Option<PushMirrorRow> = sqlx::query_as(select_push_mirror!("WHERE id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        let Some(row) = row else { return Ok(None) };
        let secret: Option<String> =
            sqlx::query_scalar("SELECT secret FROM repo_push_mirrors WHERE id = ?").bind(id.0).fetch_one(&self.pool).await?;
        let secret = secret.as_deref().map(|secret| self.key.open_str(secret)).transpose()?;
        Ok(Some(PushTarget { mirror: row.into(), secret }))
    }

    /// Saves changed settings; `false` when the mirror does not belong to `repo`.
    /// [`StorageError::Conflict`] when another mirror of the repository has the URL.
    pub async fn update(&self, repo: RepoId, id: PushMirrorId, changes: &PushMirrorChanges<'_>) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE repo_push_mirrors SET url = ?, username = ?, secret = COALESCE(?, secret), enabled = ?, sync_on_push = ?, \
             updated_at = ? WHERE id = ? AND repo_id = ?",
        )
        .bind(changes.url)
        .bind(changes.username)
        .bind(self.seal(changes.secret)?)
        .bind(changes.enabled)
        .bind(changes.sync_on_push)
        .bind(Timestamp::now().seconds())
        .bind(id.0)
        .bind(repo.0)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "a push mirror to this URL"))?;
        Ok(result.rows_affected() > 0)
    }

    /// Removes mirror `id`; `false` when it does not belong to `repo`.
    pub async fn delete(&self, repo: RepoId, id: PushMirrorId) -> Result<bool, StorageError> {
        let result =
            sqlx::query("DELETE FROM repo_push_mirrors WHERE id = ? AND repo_id = ?").bind(id.0).bind(repo.0).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    /// Makes mirror `id` due now; `false` when it does not belong to `repo`.
    pub async fn schedule_now(&self, repo: RepoId, id: PushMirrorId) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE repo_push_mirrors SET next_sync_at = ? WHERE id = ? AND repo_id = ?")
            .bind(Timestamp::now().seconds())
            .bind(id.0)
            .bind(repo.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// After a push to `repo`: makes its enabled mirrors that sync on push due at `at` at
    /// the latest (a mirror already due earlier keeps its time, so a burst of pushes is
    /// sent once). Returns how many mirrors are affected.
    pub async fn schedule_after_push(&self, repo: RepoId, at: Timestamp) -> Result<u64, StorageError> {
        let result = sqlx::query(
            "UPDATE repo_push_mirrors SET next_sync_at = MIN(next_sync_at, ?) WHERE repo_id = ? AND enabled = 1 AND sync_on_push = 1",
        )
        .bind(at.seconds())
        .bind(repo.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Enabled mirrors due at `now`, longest overdue first.
    pub async fn due(&self, now: Timestamp, limit: u32) -> Result<Vec<PushMirror>, StorageError> {
        let rows: Vec<PushMirrorRow> =
            sqlx::query_as(select_push_mirror!("WHERE enabled = 1 AND next_sync_at <= ? ORDER BY next_sync_at, id LIMIT ?"))
                .bind(now.seconds())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(PushMirror::from).collect())
    }

    /// The earliest time an enabled mirror is due, if any.
    pub async fn next_due(&self) -> Result<Option<Timestamp>, StorageError> {
        let next: Option<i64> =
            sqlx::query_scalar("SELECT MIN(next_sync_at) FROM repo_push_mirrors WHERE enabled = 1").fetch_one(&self.pool).await?;
        Ok(next.map(Timestamp))
    }

    /// Claims mirror `id` for a push by moving its next time to `next`, so pushes to the
    /// repository during the attempt schedule another one ([`Self::schedule_after_push`]
    /// lowers the time again) instead of being absorbed by this one.
    pub async fn claim(&self, id: PushMirrorId, next: Timestamp) -> Result<(), StorageError> {
        sqlx::query("UPDATE repo_push_mirrors SET next_sync_at = ? WHERE id = ?")
            .bind(next.seconds())
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Records an attempt finished at `at` (with its error, if it failed). The next one is
    /// due when [`Self::claim`] said, or earlier if a push asked for it meanwhile.
    pub async fn record_attempt(&self, id: PushMirrorId, at: Timestamp, error: Option<&str>) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE repo_push_mirrors SET last_attempt_at = ?1, last_error = ?2,              last_success_at = CASE WHEN ?2 IS NULL THEN ?1 ELSE last_success_at END WHERE id = ?3",
        )
        .bind(at.seconds())
        .bind(error)
        .bind(id.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
