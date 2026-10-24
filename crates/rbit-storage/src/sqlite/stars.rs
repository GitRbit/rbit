//! Repository stars.
//!
//! Workstream D owns this file. Keep row types here rather than in `rows.rs`.

use std::collections::HashMap;

use rbit_core::{Account, AccountId, RepoId, Timestamp, repository::OwnedRepository};
use sqlx::SqlitePool;

use super::{
    repositories::readable_by,
    rows::{AccountRow, OwnedRepositoryRow},
};
use crate::StorageError;

/// An account that starred a repository, and when.
#[derive(Debug, Clone)]
pub struct Stargazer {
    pub account: Account,
    pub starred_at: Timestamp,
}

#[derive(Debug, sqlx::FromRow)]
struct StargazerRow {
    #[sqlx(flatten)]
    account: AccountRow,
    starred_at: i64,
}

/// Star count of a repository plus whether one particular account starred it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct StarCount {
    pub count: i64,
    pub starred: bool,
}

#[derive(Debug, Clone)]
pub struct StarStore {
    pool: SqlitePool,
}

impl StarStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// One indexed query (covered by `stars_by_repo`). `viewer = None` never counts as starred.
    pub async fn count(&self, repo: RepoId, viewer: Option<AccountId>) -> Result<StarCount, StorageError> {
        Ok(sqlx::query_as("SELECT COUNT(*) AS count, COALESCE(MAX(account_id = ?), 0) AS starred FROM stars WHERE repo_id = ?")
            .bind(viewer.map(|id| id.0))
            .bind(repo.0)
            .fetch_one(&self.pool)
            .await?)
    }

    /// Star counts for many repositories in one query. Repositories without stars are absent.
    pub async fn counts(&self, repos: &[RepoId]) -> Result<HashMap<RepoId, i64>, StorageError> {
        if repos.is_empty() {
            return Ok(HashMap::new());
        }
        // A JSON array of integers keeps the statement static however many ids there are.
        let ids = format!("[{}]", repos.iter().map(|id| id.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT repo_id, COUNT(*) FROM stars WHERE repo_id IN (SELECT value FROM json_each(?)) GROUP BY repo_id")
                .bind(ids)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(repo, count)| (RepoId(repo), count)).collect())
    }

    /// Idempotent: starring twice keeps the original timestamp.
    pub async fn add(&self, account: AccountId, repo: RepoId) -> Result<(), StorageError> {
        sqlx::query("INSERT INTO stars (account_id, repo_id, created_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
            .bind(account.0)
            .bind(repo.0)
            .bind(Timestamp::now().seconds())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Idempotent: removing a star that does not exist is not an error.
    pub async fn remove(&self, account: AccountId, repo: RepoId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM stars WHERE account_id = ? AND repo_id = ?").bind(account.0).bind(repo.0).execute(&self.pool).await?;
        Ok(())
    }

    /// Accounts that starred `repo`, most recent first.
    pub async fn stargazers(&self, repo: RepoId, limit: u32) -> Result<Vec<Stargazer>, StorageError> {
        let rows: Vec<StargazerRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, s.created_at AS starred_at \
             FROM stars s JOIN accounts a ON a.id = s.account_id \
             WHERE s.repo_id = ? ORDER BY s.created_at DESC, a.id DESC LIMIT ?",
        )
        .bind(repo.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|row| Ok(Stargazer { account: row.account.try_into()?, starred_at: Timestamp(row.starred_at) })).collect()
    }

    /// Repositories starred by `account` that `viewer` may read, most recently starred first.
    ///
    /// Readable follows `rbit_auth::policy::repo_access`, expressed in SQL by the shared
    /// `readable_by!` predicate so the list can be filtered without loading every repository.
    pub async fn starred_by(
        &self,
        account: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
        limit: u32,
    ) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> = sqlx::query_as(concat!(
            "SELECT r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
             r.pushed_at, (SELECT a.archived_at FROM repo_archives a WHERE a.repo_id = r.id) AS archived_at, \
             o.kind AS owner_kind, o.name AS owner_name, o.display_name AS owner_display_name, \
             o.bio AS owner_bio, o.created_at AS owner_created_at \
             FROM stars s JOIN repositories r ON r.id = s.repo_id JOIN accounts o ON o.id = r.owner_id \
             WHERE s.account_id = ?1 AND ",
            readable_by!("?2", "?3"),
            " ORDER BY s.created_at DESC, r.id DESC LIMIT ?4"
        ))
        .bind(account.0)
        .bind(viewer.map(|id| id.0))
        .bind(viewer_is_admin)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }
}
