//! Repository collaborators and the permission each one has.
//!
//! Milestone 2 workstream A (access) owns this file. Keep row types here rather than in `rows.rs`.

use rbit_core::{
    AccountId, OrgRole, RepoId, Timestamp,
    membership::{BasePermission, Collaborator, Permission},
};
use sqlx::{SqliteExecutor, SqlitePool};

use super::rows::AccountRow;
use crate::StorageError;

/// What one viewer was granted on one repository, as stored (see
/// `RepositoryStore::grants`). The repository service turns it into
/// `rbit_auth::policy::Grants`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewerGrants {
    /// The viewer's role in the owning account, if it is an organisation they belong to.
    pub org_role: Option<OrgRole>,
    /// The owning organisation's base permission (meaningless for personal repositories).
    pub org_base: BasePermission,
    /// The best permission among the viewer's teams (of the owning organisation) on the repository.
    pub team: Option<Permission>,
    pub collaborator: Option<Permission>,
}

pub(crate) fn parse_permission(value: &str) -> Result<Permission, StorageError> {
    Permission::parse(value).ok_or_else(|| StorageError::Corrupt(format!("permission: {value}")))
}

#[derive(Debug, sqlx::FromRow)]
struct CollaboratorRow {
    #[sqlx(flatten)]
    account: AccountRow,
    permission: String,
    added_at: i64,
}

impl TryFrom<CollaboratorRow> for Collaborator {
    type Error = StorageError;

    fn try_from(row: CollaboratorRow) -> Result<Self, Self::Error> {
        Ok(Self { user: row.account.try_into()?, permission: parse_permission(&row.permission)?, added_at: Timestamp(row.added_at) })
    }
}

/// Inserts or updates one collaborator (keeping the original `added_by` and date).
pub(crate) async fn upsert<'e>(
    executor: impl SqliteExecutor<'e>,
    repo: RepoId,
    user: AccountId,
    permission: Permission,
    added_by: Option<AccountId>,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO collaborators (repo_id, user_id, permission, added_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT (repo_id, user_id) DO UPDATE SET permission = ?3",
    )
    .bind(repo.0)
    .bind(user.0)
    .bind(permission.as_str())
    .bind(added_by.map(|id| id.0))
    .bind(Timestamp::now().seconds())
    .execute(executor)
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CollaboratorStore {
    pool: SqlitePool,
}

impl CollaboratorStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Collaborators of `repo`, by name.
    pub async fn list(&self, repo: RepoId) -> Result<Vec<Collaborator>, StorageError> {
        let rows: Vec<CollaboratorRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, c.permission, c.created_at AS added_at \
             FROM collaborators c JOIN accounts a ON a.id = c.user_id \
             WHERE c.repo_id = ? ORDER BY a.name COLLATE NOCASE",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Collaborator::try_from).collect()
    }

    /// `user`'s permission on `repo`, if they are a collaborator.
    pub async fn permission(&self, repo: RepoId, user: AccountId) -> Result<Option<Permission>, StorageError> {
        let value: Option<String> = sqlx::query_scalar("SELECT permission FROM collaborators WHERE repo_id = ? AND user_id = ?")
            .bind(repo.0)
            .bind(user.0)
            .fetch_optional(&self.pool)
            .await?;
        value.as_deref().map(parse_permission).transpose()
    }

    /// Adds `user` with `permission`. [`StorageError::Conflict`] (`"collaborator"`) if
    /// they already are one.
    pub async fn add(&self, repo: RepoId, user: AccountId, permission: Permission, added_by: AccountId) -> Result<(), StorageError> {
        sqlx::query("INSERT INTO collaborators (repo_id, user_id, permission, added_by, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(repo.0)
            .bind(user.0)
            .bind(permission.as_str())
            .bind(added_by.0)
            .bind(Timestamp::now().seconds())
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::from_insert(e, "collaborator"))?;
        Ok(())
    }

    /// Changes an existing collaborator's permission. Returns whether they were one.
    pub async fn set_permission(&self, repo: RepoId, user: AccountId, permission: Permission) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE collaborators SET permission = ? WHERE repo_id = ? AND user_id = ?")
            .bind(permission.as_str())
            .bind(repo.0)
            .bind(user.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Removes a collaborator. Returns whether they were one.
    pub async fn remove(&self, repo: RepoId, user: AccountId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM collaborators WHERE repo_id = ? AND user_id = ?")
            .bind(repo.0)
            .bind(user.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
