//! Runners and their registration tokens (migration `0044_actions.sql`).
//!
//! Tokens of both kinds are stored only as SHA-256 digests and looked up by digest through a
//! unique index. Registration consumes one use of a registration token and creates the
//! runner in the same transaction, so a single-use token registers exactly one runner.
//!
//! The Actions foundation (workstream F) owns this file.

use rbit_core::{
    AccountId, RepoId, Timestamp,
    actions::{RegistrationToken, RegistrationTokenId, Runner, RunnerId, RunnerLabel, RunnerScope},
};
use sqlx::SqlitePool;

use super::actions_rows::{bytes, labels, labels_json, runner_columns, runner_scope, timestamp};
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct RunnerRow {
    id: i64,
    uuid: String,
    name: String,
    scope_kind: String,
    owner_id: Option<i64>,
    repo_id: Option<i64>,
    labels: String,
    pinned_labels: Option<String>,
    version: String,
    ephemeral: bool,
    disabled: bool,
    last_online: Option<i64>,
    created_at: i64,
}

impl TryFrom<RunnerRow> for Runner {
    type Error = StorageError;

    fn try_from(row: RunnerRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: RunnerId(row.id),
            uuid: row.uuid,
            name: row.name,
            scope: runner_scope(&row.scope_kind, row.owner_id, row.repo_id)?,
            labels: labels(&row.labels)?,
            pinned_labels: row.pinned_labels.as_deref().map(labels).transpose()?,
            version: row.version,
            ephemeral: row.ephemeral,
            disabled: row.disabled,
            last_online: timestamp(row.last_online),
            created_at: Timestamp(row.created_at),
        })
    }
}

macro_rules! select_runner {
    ($extra:literal, $clause:literal) => {
        concat!(
            "SELECT id, uuid, name, scope_kind, owner_id, repo_id, labels, pinned_labels, version, ephemeral, disabled, \
             last_online, created_at",
            $extra,
            " FROM actions_runners ",
            $clause
        )
    };
    ($clause:literal) => {
        select_runner!("", $clause)
    };
}

/// A runner row and the digest of its credential (`select_runner!(", token_digest", …)`).
#[derive(Debug, sqlx::FromRow)]
struct RunnerWithDigest {
    #[sqlx(flatten)]
    runner: RunnerRow,
    token_digest: Vec<u8>,
}

#[derive(Debug, sqlx::FromRow)]
struct TokenRow {
    id: i64,
    scope_kind: String,
    owner_id: Option<i64>,
    repo_id: Option<i64>,
    labels: Option<String>,
    uses_remaining: Option<i64>,
    created_by: Option<i64>,
    created_at: i64,
    expires_at: i64,
    revoked_at: Option<i64>,
}

impl TryFrom<TokenRow> for RegistrationToken {
    type Error = StorageError;

    fn try_from(row: TokenRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: RegistrationTokenId(row.id),
            scope: runner_scope(&row.scope_kind, row.owner_id, row.repo_id)?,
            labels: row.labels.as_deref().map(labels).transpose()?,
            uses_remaining: row.uses_remaining,
            created_by: row.created_by.map(AccountId),
            created_at: Timestamp(row.created_at),
            expires_at: Timestamp(row.expires_at),
            revoked_at: timestamp(row.revoked_at),
        })
    }
}

macro_rules! select_token {
    ($clause:expr) => {
        concat!(
            "SELECT id, scope_kind, owner_id, repo_id, labels, uses_remaining, created_by, created_at, expires_at, revoked_at \
             FROM actions_registration_tokens ",
            $clause
        )
    };
}

/// A registration token to store.
#[derive(Debug, Clone)]
pub struct NewRegistrationToken<'a> {
    pub digest: &'a [u8; 32],
    pub scope: RunnerScope,
    pub labels: Option<&'a [RunnerLabel]>,
    /// `None`: unlimited.
    pub uses: Option<u32>,
    pub created_by: Option<AccountId>,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

/// A runner registering with a registration token.
#[derive(Debug, Clone)]
pub struct NewRunner<'a> {
    /// The digest of the registration token presented.
    pub registration_digest: &'a [u8; 32],
    pub uuid: &'a str,
    /// The digest of the runner's own new token.
    pub token_digest: &'a [u8; 32],
    pub name: &'a str,
    pub version: &'a str,
    pub labels: &'a [RunnerLabel],
    pub ephemeral: bool,
    pub now: Timestamp,
}

/// Why a registration was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationRefusal {
    /// No usable token has that digest (unknown, revoked, expired or used up).
    InvalidToken,
    /// The token pins labels and the runner asked for others.
    LabelsNotAllowed,
}

#[derive(Debug, Clone)]
pub struct ActionsRunnerStore {
    pool: SqlitePool,
}

impl ActionsRunnerStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_registration_token(&self, new: &NewRegistrationToken<'_>) -> Result<RegistrationToken, StorageError> {
        let (kind, owner, repo) = runner_columns(new.scope);
        let labels = new.labels.map(labels_json).transpose()?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO actions_registration_tokens \
             (digest, scope_kind, owner_id, repo_id, labels, uses_remaining, created_by, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(&new.digest[..])
        .bind(kind)
        .bind(owner)
        .bind(repo)
        .bind(labels)
        .bind(new.uses.map(i64::from))
        .bind(new.created_by.map(|by| by.0))
        .bind(new.created_at.0)
        .bind(new.expires_at.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| StorageError::from_insert(error, "registration token"))?;
        let row: TokenRow = sqlx::query_as(select_token!("WHERE id = ?")).bind(id).fetch_one(&self.pool).await?;
        RegistrationToken::try_from(row)
    }

    /// The registration tokens of exactly `scope`, newest first.
    pub async fn registration_tokens(&self, scope: RunnerScope) -> Result<Vec<RegistrationToken>, StorageError> {
        let (kind, owner, repo) = runner_columns(scope);
        let rows: Vec<TokenRow> = sqlx::query_as(select_token!("WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ? ORDER BY id DESC"))
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(RegistrationToken::try_from).collect()
    }

    /// Revokes token `id` of `scope`. False when there is no such token in that scope.
    pub async fn revoke_registration_token(
        &self,
        scope: RunnerScope,
        id: RegistrationTokenId,
        now: Timestamp,
    ) -> Result<bool, StorageError> {
        let (kind, owner, repo) = runner_columns(scope);
        let result = sqlx::query(
            "UPDATE actions_registration_tokens SET revoked_at = COALESCE(revoked_at, ?) \
             WHERE id = ? AND scope_kind = ? AND owner_id IS ? AND repo_id IS ?",
        )
        .bind(now.0)
        .bind(id.0)
        .bind(kind)
        .bind(owner)
        .bind(repo)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Deletes registration tokens that expired or were revoked before `before`.
    pub async fn delete_stale_registration_tokens(&self, before: Timestamp) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM actions_registration_tokens WHERE expires_at < ?1 OR revoked_at < ?1")
            .bind(before.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Registers a runner with a registration token: one use of the token is consumed and
    /// the runner created in its scope, atomically.
    pub async fn register(&self, new: &NewRunner<'_>) -> Result<Result<Runner, RegistrationRefusal>, StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let token: Option<TokenRow> = sqlx::query_as(select_token!(
            "WHERE digest = ?1 AND revoked_at IS NULL AND expires_at > ?2 AND (uses_remaining IS NULL OR uses_remaining > 0)"
        ))
        .bind(&new.registration_digest[..])
        .bind(new.now.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(token) = token.map(RegistrationToken::try_from).transpose()? else {
            return Ok(Err(RegistrationRefusal::InvalidToken));
        };
        if let Some(allowed) = &token.labels
            && new.labels.iter().any(|label| !allowed.contains(label))
        {
            return Ok(Err(RegistrationRefusal::LabelsNotAllowed));
        }
        sqlx::query(
            "UPDATE actions_registration_tokens SET uses_remaining = uses_remaining - 1 WHERE id = ? AND uses_remaining IS NOT NULL",
        )
        .bind(token.id.0)
        .execute(&mut *tx)
        .await?;
        let (kind, owner, repo) = runner_columns(token.scope);
        let pinned = token.labels.as_deref().map(labels_json).transpose()?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO actions_runners (uuid, token_digest, name, scope_kind, owner_id, repo_id, labels, pinned_labels, version, \
             ephemeral, last_online, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11) RETURNING id",
        )
        .bind(new.uuid)
        .bind(&new.token_digest[..])
        .bind(new.name)
        .bind(kind)
        .bind(owner)
        .bind(repo)
        .bind(labels_json(new.labels)?)
        .bind(pinned)
        .bind(new.version)
        .bind(new.ephemeral)
        .bind(new.now.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| StorageError::from_insert(error, "runner"))?;
        let row: RunnerRow = sqlx::query_as(select_runner!("WHERE id = ?")).bind(id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Runner::try_from(row).map(Ok)
    }

    /// The runner whose token has `digest`.
    pub async fn find_by_token(&self, digest: &[u8; 32]) -> Result<Option<Runner>, StorageError> {
        let row: Option<RunnerRow> =
            sqlx::query_as(select_runner!("WHERE token_digest = ?")).bind(&digest[..]).fetch_optional(&self.pool).await?;
        row.map(Runner::try_from).transpose()
    }

    pub async fn find(&self, id: RunnerId) -> Result<Option<Runner>, StorageError> {
        let row: Option<RunnerRow> = sqlx::query_as(select_runner!("WHERE id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(Runner::try_from).transpose()
    }

    /// Every runner, for the in-memory runner cache.
    pub async fn all(&self) -> Result<Vec<Runner>, StorageError> {
        let rows: Vec<RunnerRow> = sqlx::query_as(select_runner!("ORDER BY id")).fetch_all(&self.pool).await?;
        rows.into_iter().map(Runner::try_from).collect()
    }

    // Actions: S.
    /// Every runner with the digest of its credential: what the runner cache is loaded from.
    pub async fn all_with_digests(&self) -> Result<Vec<(Runner, [u8; 32])>, StorageError> {
        let rows: Vec<RunnerWithDigest> = sqlx::query_as(select_runner!(", token_digest", "ORDER BY id")).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok((Runner::try_from(row.runner)?, bytes("runner token digest", &row.token_digest)?))).collect()
    }

    /// The runners of exactly `scope`, oldest first.
    pub async fn list(&self, scope: RunnerScope) -> Result<Vec<Runner>, StorageError> {
        let (kind, owner, repo) = runner_columns(scope);
        let rows: Vec<RunnerRow> = sqlx::query_as(select_runner!("WHERE scope_kind = ? AND owner_id IS ? AND repo_id IS ? ORDER BY id"))
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Runner::try_from).collect()
    }

    /// The runners that can serve a repository of `owner`: its own, the owner's and the
    /// instance's (whether instance runners serve the owner is the caller's decision).
    pub async fn list_for_repository(&self, repo: RepoId, owner: AccountId) -> Result<Vec<Runner>, StorageError> {
        let rows: Vec<RunnerRow> =
            sqlx::query_as(select_runner!("WHERE scope_kind = 'instance' OR owner_id = ? OR repo_id = ? ORDER BY id"))
                .bind(owner.0)
                .bind(repo.0)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(Runner::try_from).collect()
    }

    /// Records the labels and version a runner declares.
    pub async fn declare(&self, id: RunnerId, labels: &[RunnerLabel], version: &str, now: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE actions_runners SET labels = ?, version = ?, last_online = ? WHERE id = ?")
            .bind(labels_json(labels)?)
            .bind(version)
            .bind(now.0)
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Records that runners were seen (callers coalesce these writes).
    pub async fn touch(&self, ids: &[RunnerId], now: Timestamp) -> Result<(), StorageError> {
        let ids = serde_json::to_string(&ids.iter().map(|id| id.0).collect::<Vec<_>>())
            .map_err(|error| StorageError::Corrupt(error.to_string()))?;
        sqlx::query(
            "UPDATE actions_runners SET last_online = MAX(COALESCE(last_online, 0), ?) WHERE id IN (SELECT value FROM json_each(?))",
        )
        .bind(now.0)
        .bind(ids)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Disables or re-enables runner `id` of `scope`.
    pub async fn set_disabled(&self, scope: RunnerScope, id: RunnerId, disabled: bool) -> Result<bool, StorageError> {
        let (kind, owner, repo) = runner_columns(scope);
        let result =
            sqlx::query("UPDATE actions_runners SET disabled = ? WHERE id = ? AND scope_kind = ? AND owner_id IS ? AND repo_id IS ?")
                .bind(disabled)
                .bind(id.0)
                .bind(kind)
                .bind(owner)
                .bind(repo)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Removes runner `id` of `scope`.
    pub async fn delete(&self, scope: RunnerScope, id: RunnerId) -> Result<bool, StorageError> {
        let (kind, owner, repo) = runner_columns(scope);
        let result = sqlx::query("DELETE FROM actions_runners WHERE id = ? AND scope_kind = ? AND owner_id IS ? AND repo_id IS ?")
            .bind(id.0)
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Removes an ephemeral runner after its task. Other runners are left alone.
    pub async fn delete_ephemeral(&self, id: RunnerId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM actions_runners WHERE id = ? AND ephemeral = 1").bind(id.0).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }
}
