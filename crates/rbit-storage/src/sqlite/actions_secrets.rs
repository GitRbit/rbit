//! Encrypted secrets and plain variables at owner and repository scope (migration
//! `0048_actions.sql`), and at environment scope (Actions: environments, migration `0053`),
//! which live in tables of their own and are reached through the same methods.
//!
//! The store never sees a secret's value: callers seal it (rbit-services' `actions::secrets`)
//! with associated data that binds the secret's scope and id, so the id must exist before
//! the ciphertext does. [`ActionsSecretStore::put_secret`] therefore takes a sealing closure
//! and calls it with the id inside the writing transaction.
//!
//! The Actions foundation (workstream F) owns this file.

use rbit_core::{
    AccountId, RepoId, Timestamp,
    actions::{
        EnvironmentId, SealedSecret, SecretId, SecretInfo, SecretName, SecretScope, SecretVisibility, Variable, VariableId, VariableName,
    },
};
use sqlx::SqlitePool;

use super::actions_rows::{bytes, corrupt, secret_columns, secret_scope};
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct SecretRow {
    id: i64,
    scope_kind: String,
    owner_id: Option<i64>,
    repo_id: Option<i64>,
    name: String,
    visibility: String,
    key_id: Vec<u8>,
    created_at: i64,
    updated_at: i64,
    updated_by: Option<i64>,
}

impl TryFrom<SecretRow> for SecretInfo {
    type Error = StorageError;

    fn try_from(row: SecretRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: SecretId(row.id),
            scope: secret_scope(&row.scope_kind, row.owner_id, row.repo_id)?,
            name: row.name.parse().map_err(|error| corrupt("secret name", error))?,
            visibility: SecretVisibility::parse(&row.visibility).ok_or_else(|| corrupt("secret visibility", &row.visibility))?,
            key_id: bytes("secret key id", &row.key_id)?,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            updated_by: row.updated_by.map(AccountId),
        })
    }
}

macro_rules! select_secret {
    ($clause:expr) => {
        concat!(
            "SELECT s.id, s.scope_kind, s.owner_id, s.repo_id, s.name, s.visibility, s.key_id, s.created_at, s.updated_at, s.updated_by \
             FROM actions_secrets s ",
            $clause
        )
    };
}

#[derive(Debug, sqlx::FromRow)]
struct VariableRow {
    id: i64,
    scope_kind: String,
    owner_id: Option<i64>,
    repo_id: Option<i64>,
    name: String,
    value: String,
    created_at: i64,
    updated_at: i64,
    updated_by: Option<i64>,
}

impl TryFrom<VariableRow> for Variable {
    type Error = StorageError;

    fn try_from(row: VariableRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: VariableId(row.id),
            scope: secret_scope(&row.scope_kind, row.owner_id, row.repo_id)?,
            name: row.name.parse().map_err(|error| corrupt("variable name", error))?,
            value: row.value,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            updated_by: row.updated_by.map(AccountId),
        })
    }
}

macro_rules! select_variable {
    ($clause:expr) => {
        concat!(
            "SELECT v.id, v.scope_kind, v.owner_id, v.repo_id, v.name, v.value, v.created_at, v.updated_at, v.updated_by \
             FROM actions_variables v ",
            $clause
        )
    };
}

// Actions: environments.
/// An environment secret: its info in [`SecretRow`]'s shape, and its sealed value.
#[derive(Debug, sqlx::FromRow)]
struct EnvironmentSecretRow {
    #[sqlx(flatten)]
    info: EnvironmentSecretInfo,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentSecretInfo {
    id: i64,
    environment_id: i64,
    name: String,
    key_id: Vec<u8>,
    created_at: i64,
    updated_at: i64,
    updated_by: Option<i64>,
}

impl TryFrom<EnvironmentSecretInfo> for SecretInfo {
    type Error = StorageError;

    fn try_from(row: EnvironmentSecretInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            id: SecretId(row.id),
            scope: SecretScope::Environment(EnvironmentId(row.environment_id)),
            name: row.name.parse().map_err(|error| corrupt("secret name", error))?,
            visibility: SecretVisibility::All,
            key_id: bytes("secret key id", &row.key_id)?,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            updated_by: row.updated_by.map(AccountId),
        })
    }
}

macro_rules! select_environment_secret {
    ($clause:expr) => {
        concat!(
            "SELECT id, environment_id, name, key_id, created_at, updated_at, updated_by, nonce, ciphertext \
             FROM actions_environment_secrets ",
            $clause
        )
    };
}

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentVariableRow {
    id: i64,
    environment_id: i64,
    name: String,
    value: String,
    created_at: i64,
    updated_at: i64,
    updated_by: Option<i64>,
}

impl TryFrom<EnvironmentVariableRow> for Variable {
    type Error = StorageError;

    fn try_from(row: EnvironmentVariableRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: VariableId(row.id),
            scope: SecretScope::Environment(EnvironmentId(row.environment_id)),
            name: row.name.parse().map_err(|error| corrupt("variable name", error))?,
            value: row.value,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            updated_by: row.updated_by.map(AccountId),
        })
    }
}

macro_rules! select_environment_variable {
    ($clause:expr) => {
        concat!("SELECT id, environment_id, name, value, created_at, updated_at, updated_by FROM actions_environment_variables ", $clause)
    };
}

#[derive(Debug, sqlx::FromRow)]
struct TaskSecretRow {
    #[sqlx(flatten)]
    secret: SecretRow,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

/// A secret a task may receive: where it comes from and its sealed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSecret {
    pub info: SecretInfo,
    pub sealed: SealedSecret,
}

#[derive(Debug, Clone)]
pub struct ActionsSecretStore {
    pool: SqlitePool,
}

impl ActionsSecretStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Creates or replaces secret `name` in `scope`, sealed by `seal` for its id. Visibility
    /// applies to owner secrets; repository secrets are always [`SecretVisibility::All`].
    pub async fn put_secret(
        &self,
        scope: SecretScope,
        name: &SecretName,
        visibility: SecretVisibility,
        by: AccountId,
        now: Timestamp,
        seal: impl FnOnce(SecretId) -> SealedSecret,
    ) -> Result<SecretInfo, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            return self.put_environment_secret(environment, name, by, now, seal).await;
        }
        let (kind, owner, repo) = secret_columns(scope);
        let visibility = if matches!(scope, SecretScope::Repository(_)) { SecretVisibility::All } else { visibility };
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT id FROM actions_secrets WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ? AND name = ?")
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .bind(name.as_str())
                .fetch_optional(&mut *tx)
                .await?;
        let id = match existing {
            Some(id) => id,
            // A placeholder of the right shape, sealed below before anyone can read it.
            None => {
                sqlx::query_scalar(
                    "INSERT INTO actions_secrets (scope_kind, owner_id, repo_id, name, visibility, key_id, nonce, ciphertext, \
                     created_at, updated_at, updated_by) VALUES (?1, ?2, ?3, ?4, ?5, zeroblob(8), zeroblob(12), zeroblob(16), ?6, ?6, ?7) \
                     RETURNING id",
                )
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .bind(name.as_str())
                .bind(visibility.as_str())
                .bind(now.0)
                .bind(by.0)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        let sealed = seal(SecretId(id));
        sqlx::query(
            "UPDATE actions_secrets SET visibility = ?, key_id = ?, nonce = ?, ciphertext = ?, updated_at = ?, updated_by = ? WHERE id = ?",
        )
        .bind(visibility.as_str())
        .bind(&sealed.key_id[..])
        .bind(&sealed.nonce[..])
        .bind(&sealed.ciphertext)
        .bind(now.0)
        .bind(by.0)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let row: SecretRow = sqlx::query_as(select_secret!("WHERE s.id = ?")).bind(id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        SecretInfo::try_from(row)
    }

    /// The secrets of exactly `scope`, by name. Values are not part of the listing.
    pub async fn secrets(&self, scope: SecretScope) -> Result<Vec<SecretInfo>, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            let rows: Vec<EnvironmentSecretRow> = sqlx::query_as(select_environment_secret!("WHERE environment_id = ? ORDER BY name"))
                .bind(environment.0)
                .fetch_all(&self.pool)
                .await?;
            return rows.into_iter().map(|row| SecretInfo::try_from(row.info)).collect();
        }
        let (kind, owner, repo) = secret_columns(scope);
        let rows: Vec<SecretRow> =
            sqlx::query_as(select_secret!("WHERE s.scope_kind = ? AND s.owner_id IS ? AND s.repo_id IS ? ORDER BY s.name"))
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(SecretInfo::try_from).collect()
    }

    pub async fn count_secrets(&self, scope: SecretScope) -> Result<u64, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actions_environment_secrets WHERE environment_id = ?")
                .bind(environment.0)
                .fetch_one(&self.pool)
                .await?;
            return Ok(u64::try_from(count).unwrap_or_default());
        }
        let (kind, owner, repo) = secret_columns(scope);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actions_secrets WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ?")
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    pub async fn delete_secret(&self, scope: SecretScope, name: &SecretName) -> Result<bool, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            let result = sqlx::query("DELETE FROM actions_environment_secrets WHERE environment_id = ? AND name = ?")
                .bind(environment.0)
                .bind(name.as_str())
                .execute(&self.pool)
                .await?;
            return Ok(result.rows_affected() == 1);
        }
        let (kind, owner, repo) = secret_columns(scope);
        let result = sqlx::query("DELETE FROM actions_secrets WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ? AND name = ?")
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .bind(name.as_str())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Replaces the repositories an owner secret with selected visibility is given to.
    /// Repositories of other owners are ignored. False when there is no such owner secret.
    pub async fn set_secret_repositories(&self, owner: AccountId, name: &SecretName, repos: &[RepoId]) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM actions_secrets WHERE scope_kind = 'owner' AND owner_id = ? AND name = ?")
            .bind(owner.0)
            .bind(name.as_str())
            .fetch_optional(&mut *tx)
            .await?;
        let Some(id) = id else {
            return Ok(false);
        };
        sqlx::query("DELETE FROM actions_secret_repositories WHERE secret_id = ?").bind(id).execute(&mut *tx).await?;
        for repo in repos {
            sqlx::query(
                "INSERT INTO actions_secret_repositories (secret_id, repo_id) SELECT ?1, id FROM repositories WHERE id = ?2 AND owner_id = ?3 \
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(repo.0)
            .bind(owner.0)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// The repositories an owner secret is given to (meaningful with selected visibility).
    pub async fn secret_repositories(&self, id: SecretId) -> Result<Vec<RepoId>, StorageError> {
        let repos: Vec<i64> = sqlx::query_scalar("SELECT repo_id FROM actions_secret_repositories WHERE secret_id = ? ORDER BY repo_id")
            .bind(id.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(repos.into_iter().map(RepoId).collect())
    }

    /// Every secret a task of `repo` (owned by `owner`) may receive: the repository's, and
    /// the owner's that are visible to it. Repository secrets come first; on a name clash
    /// the caller keeps the repository's.
    pub async fn secrets_for_task(&self, repo: RepoId, owner: AccountId) -> Result<Vec<TaskSecret>, StorageError> {
        let rows: Vec<TaskSecretRow> = sqlx::query_as(
            "SELECT s.id, s.scope_kind, s.owner_id, s.repo_id, s.name, s.visibility, s.key_id, s.created_at, s.updated_at, s.updated_by,              s.nonce, s.ciphertext FROM actions_secrets s              WHERE s.repo_id = ?1 OR s.owner_id = ?2 AND (s.visibility = 'all' OR EXISTS                  (SELECT 1 FROM actions_secret_repositories r WHERE r.secret_id = s.id AND r.repo_id = ?1))              ORDER BY s.scope_kind = 'owner', s.name",
        )
        .bind(repo.0)
        .bind(owner.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let info = SecretInfo::try_from(row.secret)?;
                let sealed = SealedSecret { key_id: info.key_id, nonce: bytes("secret nonce", &row.nonce)?, ciphertext: row.ciphertext };
                Ok(TaskSecret { info, sealed })
            })
            .collect()
    }

    /// Whether any secret is stored (the key file must then not be created anew).
    pub async fn any_secret(&self) -> Result<bool, StorageError> {
        let any: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM actions_secrets UNION ALL SELECT 1 FROM actions_environment_secrets LIMIT 1")
                .fetch_optional(&self.pool)
                .await?;
        Ok(any.is_some())
    }

    // Actions: environments.
    /// The secrets of `environment`, with their sealed values, for a task that runs in it.
    pub async fn secrets_for_environment(&self, environment: EnvironmentId) -> Result<Vec<TaskSecret>, StorageError> {
        let rows: Vec<EnvironmentSecretRow> = sqlx::query_as(select_environment_secret!("WHERE environment_id = ? ORDER BY name"))
            .bind(environment.0)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                let info = SecretInfo::try_from(row.info)?;
                let sealed = SealedSecret { key_id: info.key_id, nonce: bytes("secret nonce", &row.nonce)?, ciphertext: row.ciphertext };
                Ok(TaskSecret { info, sealed })
            })
            .collect()
    }

    async fn put_environment_secret(
        &self,
        environment: EnvironmentId,
        name: &SecretName,
        by: AccountId,
        now: Timestamp,
        seal: impl FnOnce(SecretId) -> SealedSecret,
    ) -> Result<SecretInfo, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM actions_environment_secrets WHERE environment_id = ? AND name = ?")
            .bind(environment.0)
            .bind(name.as_str())
            .fetch_optional(&mut *tx)
            .await?;
        let id = match existing {
            Some(id) => id,
            // A placeholder of the right shape, sealed below before anyone can read it.
            None => {
                sqlx::query_scalar(
                    "INSERT INTO actions_environment_secrets (environment_id, name, key_id, nonce, ciphertext, created_at, updated_at, \
                     updated_by) VALUES (?1, ?2, zeroblob(8), zeroblob(12), zeroblob(16), ?3, ?3, ?4) RETURNING id",
                )
                .bind(environment.0)
                .bind(name.as_str())
                .bind(now.0)
                .bind(by.0)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        let sealed = seal(SecretId(id));
        sqlx::query(
            "UPDATE actions_environment_secrets SET key_id = ?, nonce = ?, ciphertext = ?, updated_at = ?, updated_by = ? WHERE id = ?",
        )
        .bind(&sealed.key_id[..])
        .bind(&sealed.nonce[..])
        .bind(&sealed.ciphertext)
        .bind(now.0)
        .bind(by.0)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let row: EnvironmentSecretRow = sqlx::query_as(select_environment_secret!("WHERE id = ?")).bind(id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        SecretInfo::try_from(row.info)
    }

    /// Creates or replaces variable `name` in `scope`.
    pub async fn put_variable(
        &self,
        scope: SecretScope,
        name: &VariableName,
        value: &str,
        by: AccountId,
        now: Timestamp,
    ) -> Result<Variable, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            sqlx::query(
                "INSERT INTO actions_environment_variables (environment_id, name, value, created_at, updated_at, updated_by) \
                 VALUES (?1, ?2, ?3, ?4, ?4, ?5) ON CONFLICT (environment_id, name) DO UPDATE SET value = ?3, updated_at = ?4, updated_by = ?5",
            )
            .bind(environment.0)
            .bind(name.as_str())
            .bind(value)
            .bind(now.0)
            .bind(by.0)
            .execute(&self.pool)
            .await?;
            let row: EnvironmentVariableRow = sqlx::query_as(select_environment_variable!("WHERE environment_id = ? AND name = ?"))
                .bind(environment.0)
                .bind(name.as_str())
                .fetch_one(&self.pool)
                .await?;
            return Variable::try_from(row);
        }
        let (kind, owner, repo) = secret_columns(scope);
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated =
            sqlx::query("UPDATE actions_variables SET value = ?, updated_at = ?, updated_by = ? WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ? AND name = ?")
                .bind(value)
                .bind(now.0)
                .bind(by.0)
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .bind(name.as_str())
                .execute(&mut *tx)
                .await?;
        if updated.rows_affected() == 0 {
            sqlx::query(
                "INSERT INTO actions_variables (scope_kind, owner_id, repo_id, name, value, created_at, updated_at, updated_by) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
            )
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .bind(name.as_str())
            .bind(value)
            .bind(now.0)
            .bind(by.0)
            .execute(&mut *tx)
            .await?;
        }
        let row: VariableRow =
            sqlx::query_as(select_variable!("WHERE v.scope_kind = ? AND v.owner_id IS ? AND v.repo_id IS ? AND v.name = ?"))
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .bind(name.as_str())
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Variable::try_from(row)
    }

    /// The variables of exactly `scope`, by name.
    pub async fn variables(&self, scope: SecretScope) -> Result<Vec<Variable>, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            let rows: Vec<EnvironmentVariableRow> = sqlx::query_as(select_environment_variable!("WHERE environment_id = ? ORDER BY name"))
                .bind(environment.0)
                .fetch_all(&self.pool)
                .await?;
            return rows.into_iter().map(Variable::try_from).collect();
        }
        let (kind, owner, repo) = secret_columns(scope);
        let rows: Vec<VariableRow> =
            sqlx::query_as(select_variable!("WHERE v.scope_kind = ? AND v.owner_id IS ? AND v.repo_id IS ? ORDER BY v.name"))
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(Variable::try_from).collect()
    }

    pub async fn count_variables(&self, scope: SecretScope) -> Result<u64, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actions_environment_variables WHERE environment_id = ?")
                .bind(environment.0)
                .fetch_one(&self.pool)
                .await?;
            return Ok(u64::try_from(count).unwrap_or_default());
        }
        let (kind, owner, repo) = secret_columns(scope);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM actions_variables WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ?")
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .fetch_one(&self.pool)
                .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    pub async fn delete_variable(&self, scope: SecretScope, name: &VariableName) -> Result<bool, StorageError> {
        if let SecretScope::Environment(environment) = scope {
            let result = sqlx::query("DELETE FROM actions_environment_variables WHERE environment_id = ? AND name = ?")
                .bind(environment.0)
                .bind(name.as_str())
                .execute(&self.pool)
                .await?;
            return Ok(result.rows_affected() == 1);
        }
        let (kind, owner, repo) = secret_columns(scope);
        let result = sqlx::query("DELETE FROM actions_variables WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ? AND name = ?")
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .bind(name.as_str())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// The variables a task of `repo` (owned by `owner`) receives: the repository's first,
    /// then the owner's; on a name clash the caller keeps the repository's.
    pub async fn variables_for_task(&self, repo: RepoId, owner: AccountId) -> Result<Vec<Variable>, StorageError> {
        let rows: Vec<VariableRow> =
            sqlx::query_as(select_variable!("WHERE v.repo_id = ? OR v.owner_id = ? ORDER BY v.scope_kind = 'owner', v.name"))
                .bind(repo.0)
                .bind(owner.0)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(Variable::try_from).collect()
    }
}
