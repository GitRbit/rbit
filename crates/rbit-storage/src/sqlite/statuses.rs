//! Commit statuses: the reports external CI posts about a commit.
//!
//! Milestone 3 workstream S (commit statuses and required checks) owns this file. Keep
//! row types here rather than in `rows.rs`.
//!
//! Every report is a row; the current status of a `(repository, commit, context)` is the
//! row with the highest id. [`CommitStatusStore::create`] enforces the per-triple cap in
//! the same statement as the insert. Validation of the values happens in the service
//! (`rbit_core::status::StatusReport`).

use rbit_core::{
    AccountId, RepoId, Timestamp,
    status::{CommitStatus, StatusContext, StatusId, StatusReport, StatusState},
};
use sqlx::SqlitePool;

use crate::StorageError;

/// The columns of a status row, in [`StatusRow`]'s order.
macro_rules! status_columns {
    () => {
        "id, repo_id, sha, state, context, description, target_url, creator_id, created_at, updated_at"
    };
}

/// Only the latest row of each context: no newer row exists for the same triple.
macro_rules! latest_only {
    () => {
        "NOT EXISTS (SELECT 1 FROM commit_statuses n WHERE n.repo_id = s.repo_id AND n.sha = s.sha \
         AND n.context = s.context AND n.id > s.id)"
    };
}

#[derive(Debug, sqlx::FromRow)]
struct StatusRow {
    id: i64,
    repo_id: i64,
    sha: String,
    state: String,
    context: String,
    description: Option<String>,
    target_url: Option<String>,
    creator_id: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<StatusRow> for CommitStatus {
    type Error = StorageError;

    fn try_from(row: StatusRow) -> Result<Self, Self::Error> {
        let corrupt = |what: &str, detail: &str| StorageError::Corrupt(format!("commit status {what}: {detail}"));
        Ok(Self {
            id: StatusId(row.id),
            repo_id: RepoId(row.repo_id),
            state: StatusState::parse(&row.state).ok_or_else(|| corrupt("state", &row.state))?,
            context: StatusContext::parse(&row.context).map_err(|error| corrupt("context", &error.to_string()))?,
            sha: row.sha,
            description: row.description,
            target_url: row.target_url,
            creator: row.creator_id.map(AccountId),
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        })
    }
}

/// A JSON array of strings for `json_each(?)`.
fn json_strings(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

#[derive(Debug, Clone)]
pub struct CommitStatusStore {
    pool: SqlitePool,
}

impl CommitStatusStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Stores `report` for commit `sha` (a full, lower-case id) of `repo`. `None` (and
    /// nothing stored) when the commit already has `max_per_context` reports for this
    /// context.
    pub async fn create(
        &self,
        repo: RepoId,
        sha: &str,
        report: &StatusReport,
        creator: AccountId,
        max_per_context: u32,
    ) -> Result<Option<CommitStatus>, StorageError> {
        let now = Timestamp::now().seconds();
        let row: Option<StatusRow> = sqlx::query_as(concat!(
            "INSERT INTO commit_statuses (repo_id, sha, state, context, description, target_url, creator_id, created_at, updated_at) \
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8 \
             WHERE (SELECT COUNT(*) FROM commit_statuses WHERE repo_id = ?1 AND sha = ?2 AND context = ?4) < ?9 RETURNING ",
            status_columns!()
        ))
        .bind(repo.0)
        .bind(sha)
        .bind(report.state.as_str())
        .bind(report.context.as_str())
        .bind(report.description.as_deref())
        .bind(report.target_url.as_deref())
        .bind(creator.0)
        .bind(now)
        .bind(i64::from(max_per_context))
        .fetch_optional(&self.pool)
        .await?;
        row.map(CommitStatus::try_from).transpose()
    }

    /// Report `id` of `repo`.
    pub async fn find(&self, repo: RepoId, id: StatusId) -> Result<Option<CommitStatus>, StorageError> {
        let row: Option<StatusRow> =
            sqlx::query_as(concat!("SELECT ", status_columns!(), " FROM commit_statuses WHERE id = ? AND repo_id = ?"))
                .bind(id.0)
                .bind(repo.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(CommitStatus::try_from).transpose()
    }

    /// Every report on `sha`, newest first, skipping `offset` and returning at most `limit`.
    pub async fn list(&self, repo: RepoId, sha: &str, offset: u64, limit: u32) -> Result<Vec<CommitStatus>, StorageError> {
        let rows: Vec<StatusRow> = sqlx::query_as(concat!(
            "SELECT ",
            status_columns!(),
            " FROM commit_statuses WHERE repo_id = ? AND sha = ? ORDER BY id DESC LIMIT ? OFFSET ?"
        ))
        .bind(repo.0)
        .bind(sha)
        .bind(i64::from(limit))
        .bind(i64::try_from(offset).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(CommitStatus::try_from).collect()
    }

    /// How many reports `sha` has.
    pub async fn count(&self, repo: RepoId, sha: &str) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commit_statuses WHERE repo_id = ? AND sha = ?")
            .bind(repo.0)
            .bind(sha)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// The current status of each context of `sha`, by context.
    pub async fn latest(&self, repo: RepoId, sha: &str) -> Result<Vec<CommitStatus>, StorageError> {
        let rows: Vec<StatusRow> = sqlx::query_as(concat!(
            "SELECT ",
            status_columns!(),
            " FROM commit_statuses s WHERE s.repo_id = ? AND s.sha = ? AND ",
            latest_only!(),
            " ORDER BY s.context"
        ))
        .bind(repo.0)
        .bind(sha)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(CommitStatus::try_from).collect()
    }

    /// The current status of each context of every commit in `shas` (one query), by
    /// commit and context. Commits without reports are absent.
    pub async fn latest_of(&self, repo: RepoId, shas: &[String]) -> Result<Vec<CommitStatus>, StorageError> {
        if shas.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<StatusRow> = sqlx::query_as(concat!(
            "SELECT ",
            status_columns!(),
            " FROM commit_statuses s WHERE s.repo_id = ? AND s.sha IN (SELECT value FROM json_each(?)) AND ",
            latest_only!(),
            " ORDER BY s.sha, s.context"
        ))
        .bind(repo.0)
        .bind(json_strings(shas))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(CommitStatus::try_from).collect()
    }
}
