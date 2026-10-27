//! Deployment environments (Actions: environments, migration `0053`): a repository's named
//! environments with their reviewers and branch policy. Their secrets and variables are
//! [`super::actions_secrets`]' at `SecretScope::Environment`.

use rbit_core::{
    AccountId, RepoId, Timestamp,
    actions::{BranchPolicy, Environment, EnvironmentId},
};
use sqlx::SqlitePool;

use super::actions_rows::corrupt;
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentRow {
    id: i64,
    repo_id: i64,
    name: String,
    prevent_self_review: bool,
    branch_policy: String,
    branch_patterns: String,
    created_at: i64,
    updated_at: i64,
    reviewers: String,
}

impl TryFrom<EnvironmentRow> for Environment {
    type Error = StorageError;

    fn try_from(row: EnvironmentRow) -> Result<Self, Self::Error> {
        let patterns: Vec<String> = serde_json::from_str(&row.branch_patterns).map_err(|error| corrupt("branch patterns", error))?;
        let branch_policy = match row.branch_policy.as_str() {
            "all" => BranchPolicy::All,
            "protected" => BranchPolicy::Protected,
            "selected" => BranchPolicy::Selected(patterns),
            other => return Err(corrupt("branch policy", other)),
        };
        let reviewers: Vec<i64> = serde_json::from_str(&row.reviewers).map_err(|error| corrupt("environment reviewers", error))?;
        Ok(Self {
            id: EnvironmentId(row.id),
            repo_id: RepoId(row.repo_id),
            name: row.name,
            reviewers: reviewers.into_iter().map(AccountId).collect(),
            prevent_self_review: row.prevent_self_review,
            branch_policy,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        })
    }
}

macro_rules! select_environment {
    ($clause:expr) => {
        concat!(
            "SELECT e.id, e.repo_id, e.name, e.prevent_self_review, e.branch_policy, e.branch_patterns, e.created_at, e.updated_at, \
             (SELECT COALESCE(json_group_array(r.account_id), '[]') FROM \
              (SELECT account_id FROM actions_environment_reviewers WHERE environment_id = e.id ORDER BY account_id) r) AS reviewers \
             FROM actions_environments e ",
            $clause
        )
    };
}

/// Most environments a repository may have.
pub const MAX_ENVIRONMENTS: u64 = 100;
/// Most reviewers an environment may have.
pub const MAX_REVIEWERS: usize = 6;

/// What a repository administrator sets on an environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentChanges<'a> {
    pub reviewers: &'a [AccountId],
    pub prevent_self_review: bool,
    pub branch_policy: &'a BranchPolicy,
}

#[derive(Debug, Clone)]
pub struct ActionsEnvironmentStore {
    pool: SqlitePool,
}

impl ActionsEnvironmentStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The environments of `repo`, by name.
    pub async fn list(&self, repo: RepoId) -> Result<Vec<Environment>, StorageError> {
        let rows: Vec<EnvironmentRow> =
            sqlx::query_as(select_environment!("WHERE e.repo_id = ? ORDER BY e.name")).bind(repo.0).fetch_all(&self.pool).await?;
        rows.into_iter().map(Environment::try_from).collect()
    }

    /// Environment `name` of `repo` (names compare without case).
    pub async fn find(&self, repo: RepoId, name: &str) -> Result<Option<Environment>, StorageError> {
        let row: Option<EnvironmentRow> = sqlx::query_as(select_environment!("WHERE e.repo_id = ? AND e.name = ?"))
            .bind(repo.0)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Environment::try_from).transpose()
    }

    /// Creates environment `name` of `repo` unless it exists, then applies `changes`; the
    /// environment. Reviewers are kept only if they are accounts. At most
    /// [`MAX_ENVIRONMENTS`] per repository: [`StorageError::Conflict`] past it.
    pub async fn put(
        &self,
        repo: RepoId,
        name: &str,
        changes: &EnvironmentChanges<'_>,
        now: Timestamp,
    ) -> Result<Environment, StorageError> {
        let patterns = match changes.branch_policy {
            BranchPolicy::Selected(patterns) => serde_json::to_string(patterns).map_err(|error| corrupt("branch patterns", error))?,
            _ => "[]".to_owned(),
        };
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM actions_environments WHERE repo_id = ? AND name = ?")
            .bind(repo.0)
            .bind(name)
            .fetch_optional(&mut *tx)
            .await?;
        let id = if let Some(id) = existing {
            sqlx::query(
                "UPDATE actions_environments SET prevent_self_review = ?, branch_policy = ?, branch_patterns = ?, updated_at = ? WHERE id = ?",
            )
            .bind(changes.prevent_self_review)
            .bind(changes.branch_policy.as_str())
            .bind(&patterns)
            .bind(now.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            id
        } else {
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM actions_environments WHERE repo_id = ?").bind(repo.0).fetch_one(&mut *tx).await?;
            if u64::try_from(count).unwrap_or(u64::MAX) >= MAX_ENVIRONMENTS {
                return Err(StorageError::Conflict("a repository has at most 100 environments"));
            }
            sqlx::query_scalar(
                "INSERT INTO actions_environments (repo_id, name, prevent_self_review, branch_policy, branch_patterns, created_at, \
                 updated_at) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(repo.0)
            .bind(name)
            .bind(changes.prevent_self_review)
            .bind(changes.branch_policy.as_str())
            .bind(&patterns)
            .bind(now.0)
            .bind(now.0)
            .fetch_one(&mut *tx)
            .await?
        };
        sqlx::query("DELETE FROM actions_environment_reviewers WHERE environment_id = ?").bind(id).execute(&mut *tx).await?;
        for reviewer in changes.reviewers.iter().take(MAX_REVIEWERS) {
            sqlx::query(
                "INSERT INTO actions_environment_reviewers (environment_id, account_id) SELECT ?1, id FROM accounts WHERE id = ?2 \
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(reviewer.0)
            .execute(&mut *tx)
            .await?;
        }
        let row: EnvironmentRow = sqlx::query_as(select_environment!("WHERE e.id = ?")).bind(id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Environment::try_from(row)
    }

    /// Deletes environment `name` of `repo` with its reviewers, secrets and variables. Jobs
    /// waiting for its reviewers then run without review the next time they are settled; a
    /// job that names it again recreates it, unprotected.
    pub async fn delete(&self, repo: RepoId, name: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM actions_environments WHERE repo_id = ? AND name = ?")
            .bind(repo.0)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }
}
