//! Personal access tokens. Only the SHA-256 digest of each secret is stored.

use rustit_core::{AccessToken, AccessTokenId, AccountId, Timestamp, TokenScope, User};
use sqlx::SqlitePool;

use super::rows::{AccessTokenOwnerRow, AccessTokenRow};
use crate::StorageError;

/// `last_used_at` is rewritten at most this often, keeping authenticated reads write-light.
const LAST_USED_RESOLUTION_SECONDS: i64 = 60 * 60;

/// Builds a static `SELECT` over access tokens with the given trailing clause.
/// SQL stays a compile-time literal, so it can never contain interpolated user input.
macro_rules! select_token {
    ($clause:literal) => {
        concat!("SELECT id, account_id, name, hint, scope, created_at, last_used_at, expires_at FROM access_tokens ", $clause)
    };
}

/// Everything needed to store a token. The secret itself never reaches this layer.
#[derive(Debug)]
pub struct NewAccessToken {
    pub account_id: AccountId,
    pub name: String,
    pub digest: [u8; 32],
    pub hint: String,
    pub scope: TokenScope,
    pub expires_at: Option<Timestamp>,
}

#[derive(Debug, Clone)]
pub struct AccessTokenStore {
    pool: SqlitePool,
}

impl AccessTokenStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, new: NewAccessToken) -> Result<AccessToken, StorageError> {
        let now = Timestamp::now().seconds();
        let row: AccessTokenRow = sqlx::query_as(
            "INSERT INTO access_tokens (account_id, name, token_digest, hint, scope, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, account_id, name, hint, scope, created_at, last_used_at, expires_at",
        )
        .bind(new.account_id.0)
        .bind(&new.name)
        .bind(new.digest.as_slice())
        .bind(&new.hint)
        .bind(new.scope.as_str())
        .bind(now)
        .bind(new.expires_at.map(Timestamp::seconds))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "token"))?;
        row.try_into()
    }

    /// Tokens owned by `account_id`, newest first.
    pub async fn list_for_account(&self, account_id: AccountId) -> Result<Vec<AccessToken>, StorageError> {
        let rows: Vec<AccessTokenRow> = sqlx::query_as(select_token!("WHERE account_id = ? ORDER BY created_at DESC, id DESC"))
            .bind(account_id.0)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(AccessToken::try_from).collect()
    }

    pub async fn count_for_account(&self, account_id: AccountId) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM access_tokens WHERE account_id = ?").bind(account_id.0).fetch_one(&self.pool).await?)
    }

    /// Deletes the token only if `account_id` owns it. Returns whether a token was removed.
    pub async fn delete(&self, account_id: AccountId, id: AccessTokenId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM access_tokens WHERE id = ? AND account_id = ?")
            .bind(id.0)
            .bind(account_id.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Resolves a secret's digest to the token and its owner. Expiry is the caller's concern.
    pub async fn find_by_digest(&self, digest: &[u8; 32]) -> Result<Option<(AccessToken, User)>, StorageError> {
        let row: Option<AccessTokenOwnerRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, l.email, l.is_admin, l.password_hash, \
             t.id AS token_id, t.name AS token_name, t.hint AS token_hint, t.scope AS token_scope, \
             t.created_at AS token_created_at, t.last_used_at AS token_last_used_at, t.expires_at AS token_expires_at \
             FROM access_tokens t JOIN accounts a ON a.id = t.account_id JOIN user_logins l ON l.account_id = a.id \
             WHERE t.token_digest = ?",
        )
        .bind(digest.as_slice())
        .fetch_optional(&self.pool)
        .await?;
        row.map(AccessTokenOwnerRow::into_token_and_user).transpose()
    }

    /// Records a use at `now`, unless one was already recorded within the last hour.
    pub async fn touch(&self, id: AccessTokenId, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE access_tokens SET last_used_at = ?1 \
             WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at <= ?1 - ?3)",
        )
        .bind(now.seconds())
        .bind(id.0)
        .bind(LAST_USED_RESOLUTION_SECONDS)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
