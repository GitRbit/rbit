//! Actions settings (migration `0048_actions.sql`): the per-repository switch and default
//! token permission, which owners instance runners serve, and the pull request authors a
//! repository trusts.
//!
//! The Actions foundation (workstream F) owns this file.

use rbit_core::{
    AccountId, RepoId, Timestamp,
    actions::{DefaultTokenPermission, RepoActionsSettings, TrustedUser},
};
use sqlx::SqlitePool;

use super::actions_rows::corrupt;
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct SettingsRow {
    repo_id: i64,
    enabled: bool,
    token_permission: String,
    updated_at: i64,
}

impl TryFrom<SettingsRow> for RepoActionsSettings {
    type Error = StorageError;

    fn try_from(row: SettingsRow) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_id: RepoId(row.repo_id),
            enabled: row.enabled,
            token_permission: DefaultTokenPermission::parse(&row.token_permission)
                .ok_or_else(|| corrupt("token permission", &row.token_permission))?,
            updated_at: Timestamp(row.updated_at),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ActionsSettingsStore {
    pool: SqlitePool,
}

impl ActionsSettingsStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The repository's settings; `None` when none were ever stored (Actions is off).
    pub async fn get(&self, repo: RepoId) -> Result<Option<RepoActionsSettings>, StorageError> {
        let row: Option<SettingsRow> =
            sqlx::query_as("SELECT repo_id, enabled, token_permission, updated_at FROM actions_repositories WHERE repo_id = ?")
                .bind(repo.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(RepoActionsSettings::try_from).transpose()
    }

    /// Whether Actions is on for the repository.
    pub async fn is_enabled(&self, repo: RepoId) -> Result<bool, StorageError> {
        let enabled: Option<bool> = sqlx::query_scalar("SELECT enabled FROM actions_repositories WHERE repo_id = ?")
            .bind(repo.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(enabled.unwrap_or(false))
    }

    /// Turns Actions on or off for the repository.
    pub async fn set_enabled(&self, repo: RepoId, enabled: bool, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_repositories (repo_id, enabled, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT (repo_id) DO UPDATE SET enabled = ?2, updated_at = ?3",
        )
        .bind(repo.0)
        .bind(enabled)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Sets the default permission of the repository's task tokens.
    pub async fn set_token_permission(&self, repo: RepoId, permission: DefaultTokenPermission, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_repositories (repo_id, token_permission, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT (repo_id) DO UPDATE SET token_permission = ?2, updated_at = ?3",
        )
        .bind(repo.0)
        .bind(permission.as_str())
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Whether an administrator allowed instance runners to take the owner's jobs.
    pub async fn instance_runners_allowed(&self, owner: AccountId) -> Result<bool, StorageError> {
        let allowed: Option<bool> = sqlx::query_scalar("SELECT instance_runners FROM actions_owner_policies WHERE owner_id = ?")
            .bind(owner.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(allowed.unwrap_or(false))
    }

    pub async fn set_instance_runners_allowed(
        &self,
        owner: AccountId,
        allowed: bool,
        by: AccountId,
        now: Timestamp,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_owner_policies (owner_id, instance_runners, updated_by, updated_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (owner_id) DO UPDATE SET instance_runners = ?2, updated_by = ?3, updated_at = ?4",
        )
        .bind(owner.0)
        .bind(allowed)
        .bind(by.0)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The owners instance runners serve, by id.
    pub async fn owners_with_instance_runners(&self) -> Result<Vec<AccountId>, StorageError> {
        let owners: Vec<i64> =
            sqlx::query_scalar("SELECT owner_id FROM actions_owner_policies WHERE instance_runners = 1 ORDER BY owner_id")
                .fetch_all(&self.pool)
                .await?;
        Ok(owners.into_iter().map(AccountId).collect())
    }

    /// Trusts `user`'s pull requests in `repo`. Returns false when already trusted.
    pub async fn trust(&self, repo: RepoId, user: AccountId, by: AccountId, now: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "INSERT INTO actions_trusted_users (repo_id, user_id, approved_by, created_at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(repo.0)
        .bind(user.0)
        .bind(by.0)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn untrust(&self, repo: RepoId, user: AccountId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM actions_trusted_users WHERE repo_id = ? AND user_id = ?")
            .bind(repo.0)
            .bind(user.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn is_trusted(&self, repo: RepoId, user: AccountId) -> Result<bool, StorageError> {
        let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM actions_trusted_users WHERE repo_id = ? AND user_id = ?")
            .bind(repo.0)
            .bind(user.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(found.is_some())
    }

    /// The repository's trusted users, oldest first.
    pub async fn trusted_users(&self, repo: RepoId) -> Result<Vec<TrustedUser>, StorageError> {
        let rows: Vec<(i64, i64, Option<i64>, i64)> = sqlx::query_as(
            "SELECT repo_id, user_id, approved_by, created_at FROM actions_trusted_users WHERE repo_id = ? ORDER BY created_at, user_id",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(repo, user, by, at)| TrustedUser {
                repo_id: RepoId(repo),
                user_id: AccountId(user),
                approved_by: by.map(AccountId),
                created_at: Timestamp(at),
            })
            .collect())
    }
}
