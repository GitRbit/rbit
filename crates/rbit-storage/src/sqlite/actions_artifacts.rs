//! Workflow artifacts: the rows of the files jobs uploaded (migration `0048_actions.sql`,
//! §12.7 of `docs/design/actions.md`). The files themselves live under
//! `<data_dir>/actions/artifacts`, written and removed by rbit-services' `actions::artifacts`,
//! which also enforces sizes and quotas.
//!
//! Workstream A (artifacts) owns this file.

use rbit_core::{
    RepoId, Timestamp,
    actions::{Artifact, ArtifactId, JobId, RunId},
};
use sqlx::SqlitePool;

use super::actions_rows::corrupt;
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct ArtifactRow {
    id: i64,
    run_id: i64,
    repo_id: i64,
    attempt: i64,
    job_id: Option<i64>,
    name: String,
    size: i64,
    sha256: Vec<u8>,
    created_at: i64,
    expires_at: i64,
}

impl TryFrom<ArtifactRow> for Artifact {
    type Error = StorageError;

    fn try_from(row: ArtifactRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: ArtifactId(row.id),
            run_id: RunId(row.run_id),
            repo_id: RepoId(row.repo_id),
            attempt: row.attempt,
            job_id: row.job_id.map(JobId),
            name: row.name,
            size: u64::try_from(row.size).map_err(|error| corrupt("artifact size", error))?,
            sha256: row.sha256.as_slice().try_into().map_err(|_| corrupt("artifact digest", "not 32 bytes"))?,
            created_at: Timestamp(row.created_at),
            expires_at: Timestamp(row.expires_at),
        })
    }
}

macro_rules! select_artifact {
    ($clause:expr) => {
        concat!("SELECT id, run_id, repo_id, attempt, job_id, name, size, sha256, created_at, expires_at FROM actions_artifacts ", $clause)
    };
}

/// A finished upload, to record.
#[derive(Debug, Clone)]
pub struct NewArtifact<'a> {
    pub run: RunId,
    pub repo: RepoId,
    pub attempt: i64,
    pub job: JobId,
    pub name: &'a str,
    pub size: u64,
    pub sha256: [u8; 32],
    pub now: Timestamp,
    pub expires_at: Timestamp,
}

/// What recording an upload did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// The new artifact, and the one of the same name it replaced (whose file goes).
    Stored { artifact: Box<Artifact>, replaced: Option<Box<Artifact>> },
    /// An artifact of that name exists in this run attempt and `overwrite` was not asked.
    Exists,
}

#[derive(Debug, Clone)]
pub struct ActionsArtifactStore {
    pool: SqlitePool,
}

impl ActionsArtifactStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Records `new`, replacing an artifact of the same name in its run attempt only when
    /// `overwrite`.
    pub async fn record(&self, new: &NewArtifact<'_>, overwrite: bool) -> Result<Recorded, StorageError> {
        let size = i64::try_from(new.size).map_err(|error| corrupt("artifact size", error))?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<ArtifactRow> = sqlx::query_as(select_artifact!("WHERE run_id = ? AND attempt = ? AND name = ?"))
            .bind(new.run.0)
            .bind(new.attempt)
            .bind(new.name)
            .fetch_optional(&mut *tx)
            .await?;
        let replaced = match existing {
            Some(_) if !overwrite => return Ok(Recorded::Exists),
            Some(row) => {
                sqlx::query("DELETE FROM actions_artifacts WHERE id = ?").bind(row.id).execute(&mut *tx).await?;
                Some(Box::new(Artifact::try_from(row)?))
            }
            None => None,
        };
        let row: ArtifactRow = sqlx::query_as(
            "INSERT INTO actions_artifacts (run_id, repo_id, attempt, job_id, name, size, sha256, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, run_id, repo_id, attempt, job_id, name, size, sha256, created_at, expires_at",
        )
        .bind(new.run.0)
        .bind(new.repo.0)
        .bind(new.attempt)
        .bind(new.job.0)
        .bind(new.name)
        .bind(size)
        .bind(&new.sha256[..])
        .bind(new.now.0)
        .bind(new.expires_at.0)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Recorded::Stored { artifact: Box::new(Artifact::try_from(row)?), replaced })
    }

    /// The artifacts of run `run` that have not expired by `now`, newest attempt first, then
    /// by name.
    pub async fn of_run(&self, run: RunId, now: Timestamp) -> Result<Vec<Artifact>, StorageError> {
        let rows: Vec<ArtifactRow> = sqlx::query_as(select_artifact!("WHERE run_id = ? AND expires_at > ? ORDER BY attempt DESC, name"))
            .bind(run.0)
            .bind(now.0)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Artifact::try_from).collect()
    }

    /// Artifact `id` of repository `repo`, unless it expired by `now`.
    pub async fn find(&self, repo: RepoId, id: ArtifactId, now: Timestamp) -> Result<Option<Artifact>, StorageError> {
        let row: Option<ArtifactRow> = sqlx::query_as(select_artifact!("WHERE repo_id = ? AND id = ? AND expires_at > ?"))
            .bind(repo.0)
            .bind(id.0)
            .bind(now.0)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Artifact::try_from).transpose()
    }

    /// Bytes of artifacts `repo` keeps (expired ones count until the sweeper removes them).
    pub async fn stored_bytes(&self, repo: RepoId) -> Result<u64, StorageError> {
        let bytes: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(size), 0) FROM actions_artifacts WHERE repo_id = ?")
            .bind(repo.0)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(bytes).unwrap_or_default())
    }

    /// Up to `limit` artifacts that expired by `now`, oldest first, for the sweeper.
    pub async fn expired(&self, now: Timestamp, limit: u32) -> Result<Vec<Artifact>, StorageError> {
        let rows: Vec<ArtifactRow> = sqlx::query_as(select_artifact!("WHERE expires_at <= ? ORDER BY expires_at LIMIT ?"))
            .bind(now.0)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Artifact::try_from).collect()
    }

    /// Deletes artifact `id`'s row; its file is the caller's.
    pub async fn delete(&self, id: ArtifactId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM actions_artifacts WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }
}
