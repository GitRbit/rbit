//! The merge queue (migration `0081_merge_queue.sql`, agent plan step 4.1): entries in line
//! per repository and base branch, oldest first.
use rbit_core::{AccountId, IssueId, RepoId, Timestamp};

use super::ActionsRunStore;
use crate::StorageError;

/// One entry of the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub id: i64,
    pub repo: RepoId,
    pub issue: IssueId,
    /// The pull request's number.
    pub number: u32,
    pub base_branch: String,
    pub head_sha: String,
    /// `merge`, `squash` or `rebase`.
    pub strategy: String,
    pub queued_by: AccountId,
    /// `waiting`, `testing`, `landed`, `failed` or `removed`.
    pub state: String,
    pub candidate_sha: Option<String>,
    pub base_sha: Option<String>,
    pub attempts: i64,
    pub reason: Option<String>,
    pub created_at: Timestamp,
}

/// A row as read.
#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    repo_id: i64,
    issue_id: i64,
    number: i64,
    base_branch: String,
    head_sha: String,
    strategy: String,
    queued_by: i64,
    state: String,
    candidate_sha: Option<String>,
    base_sha: Option<String>,
    attempts: i64,
    reason: Option<String>,
    created_at: i64,
}

impl From<Row> for QueueEntry {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            repo: RepoId(row.repo_id),
            issue: IssueId(row.issue_id),
            number: u32::try_from(row.number).unwrap_or_default(),
            base_branch: row.base_branch,
            head_sha: row.head_sha,
            strategy: row.strategy,
            queued_by: AccountId(row.queued_by),
            state: row.state,
            candidate_sha: row.candidate_sha,
            base_sha: row.base_sha,
            attempts: row.attempts,
            reason: row.reason,
            created_at: Timestamp(row.created_at),
        }
    }
}

macro_rules! select_entries {
    ($where:literal) => {
        concat!(
            "SELECT id, repo_id, issue_id, number, base_branch, head_sha, strategy, queued_by, state, candidate_sha, base_sha, attempts, reason, ",
            "created_at FROM merge_queue WHERE ",
            $where
        )
    };
}

/// What a queued pull request asks for.
#[derive(Debug, Clone)]
pub struct NewEntry<'a> {
    pub repo: RepoId,
    pub issue: IssueId,
    pub number: u32,
    pub base_branch: &'a str,
    pub head_sha: &'a str,
    pub strategy: &'a str,
    pub queued_by: AccountId,
}

impl ActionsRunStore {
    /// Queues `new`; `None` when the pull request already has a live entry.
    pub async fn queue_pull(&self, new: &NewEntry<'_>, now: Timestamp) -> Result<Option<QueueEntry>, StorageError> {
        let inserted: Option<i64> = sqlx::query_scalar(
            "INSERT INTO merge_queue (repo_id, issue_id, number, base_branch, head_sha, strategy, queued_by, state, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 'waiting', ?, ?) ON CONFLICT DO NOTHING RETURNING id",
        )
        .bind(new.repo.0)
        .bind(new.issue.0)
        .bind(new.number)
        .bind(new.base_branch)
        .bind(new.head_sha)
        .bind(new.strategy)
        .bind(new.queued_by.0)
        .bind(now.0)
        .bind(now.0)
        .fetch_optional(&self.pool)
        .await?;
        match inserted {
            Some(id) => self.queue_entry(id).await,
            None => Ok(None),
        }
    }

    /// Entry `id`.
    pub async fn queue_entry(&self, id: i64) -> Result<Option<QueueEntry>, StorageError> {
        let row: Option<Row> = sqlx::query_as(select_entries!("id = ?")).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(QueueEntry::from))
    }

    /// The newest entry of pull request `issue`, live or not.
    pub async fn queue_of_pull(&self, issue: IssueId) -> Result<Option<QueueEntry>, StorageError> {
        let row: Option<Row> =
            sqlx::query_as(select_entries!("issue_id = ? ORDER BY id DESC LIMIT 1")).bind(issue.0).fetch_optional(&self.pool).await?;
        Ok(row.map(QueueEntry::from))
    }

    /// The live entries of `repo`'s queue into `base`, oldest first.
    pub async fn queue_line(&self, repo: RepoId, base: &str) -> Result<Vec<QueueEntry>, StorageError> {
        let rows: Vec<Row> =
            sqlx::query_as(select_entries!("repo_id = ? AND base_branch = ? AND state IN ('waiting', 'testing') ORDER BY id"))
                .bind(repo.0)
                .bind(base)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(QueueEntry::from).collect())
    }

    /// Every repository and base branch with a live entry.
    pub async fn queue_lines(&self) -> Result<Vec<(RepoId, String)>, StorageError> {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT DISTINCT repo_id, base_branch FROM merge_queue WHERE state IN ('waiting', 'testing') ORDER BY repo_id")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(repo, base)| (RepoId(repo), base)).collect())
    }

    /// Entry `id` is testing `candidate`, built on `base`.
    pub async fn queue_testing(&self, id: i64, candidate: &str, base: &str, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE merge_queue SET state = 'testing', candidate_sha = ?, base_sha = ?, attempts = attempts + 1, updated_at = ? \
             WHERE id = ? AND state IN ('waiting', 'testing')",
        )
        .bind(candidate)
        .bind(base)
        .bind(now.0)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Entry `id` ends `state` (`landed`, `failed` or `removed`) for `reason`; `false` when it
    /// had ended already.
    pub async fn queue_finish(&self, id: i64, state: &str, reason: Option<&str>, now: Timestamp) -> Result<bool, StorageError> {
        let done =
            sqlx::query("UPDATE merge_queue SET state = ?, reason = ?, updated_at = ? WHERE id = ? AND state IN ('waiting', 'testing')")
                .bind(state)
                .bind(reason)
                .bind(now.0)
                .bind(id)
                .execute(&self.pool)
                .await?
                .rows_affected();
        Ok(done > 0)
    }
}
