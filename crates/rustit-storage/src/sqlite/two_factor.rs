//! TOTP secrets and hashed recovery codes.
//!
//! Milestone 1 workstream S owns this file. Keep row types here rather than in `rows.rs`.
//!
//! Every state change that must not race (confirming enrolment, accepting a time step,
//! spending a recovery code) is a single guarded statement, so two concurrent requests
//! can never both succeed with the same code.

use rustit_core::{AccountId, Timestamp};
use sqlx::SqlitePool;

use crate::StorageError;

/// A user's two-factor row: pending until `confirmed_at` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoFactorRecord {
    pub secret: Vec<u8>,
    pub created_at: Timestamp,
    pub confirmed_at: Option<Timestamp>,
    pub last_used_step: Option<u64>,
}

impl TwoFactorRecord {
    pub fn is_enabled(&self) -> bool {
        self.confirmed_at.is_some()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TwoFactorRow {
    secret: Vec<u8>,
    created_at: i64,
    confirmed_at: Option<i64>,
    last_used_step: Option<i64>,
}

impl From<TwoFactorRow> for TwoFactorRecord {
    fn from(row: TwoFactorRow) -> Self {
        Self {
            secret: row.secret,
            created_at: Timestamp(row.created_at),
            confirmed_at: row.confirmed_at.map(Timestamp),
            last_used_step: row.last_used_step.and_then(|step| u64::try_from(step).ok()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TwoFactorStore {
    pool: SqlitePool,
}

impl TwoFactorStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find(&self, account: AccountId) -> Result<Option<TwoFactorRecord>, StorageError> {
        let row: Option<TwoFactorRow> =
            sqlx::query_as("SELECT secret, created_at, confirmed_at, last_used_step FROM two_factor WHERE account_id = ?")
                .bind(account.0)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(TwoFactorRecord::from))
    }

    /// Whether the account has confirmed two-factor authentication.
    pub async fn is_enabled(&self, account: AccountId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM two_factor WHERE account_id = ? AND confirmed_at IS NOT NULL)")
            .bind(account.0)
            .fetch_one(&self.pool)
            .await?)
    }

    /// Stores a pending secret, replacing an earlier pending one. Returns `false` (and
    /// changes nothing) when two-factor authentication is already enabled.
    pub async fn begin_enrolment(&self, account: AccountId, secret: &[u8]) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "INSERT INTO two_factor (account_id, secret, created_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT (account_id) DO UPDATE SET secret = excluded.secret, created_at = excluded.created_at, last_used_step = NULL \
             WHERE two_factor.confirmed_at IS NULL",
        )
        .bind(account.0)
        .bind(secret)
        .bind(Timestamp::now().seconds())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Enables a pending enrolment, records the confirming code's step and replaces the
    /// recovery codes. `false` if there was no pending enrolment.
    pub async fn confirm(&self, account: AccountId, step: u64, recovery_digests: &[[u8; 32]]) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let confirmed =
            sqlx::query("UPDATE two_factor SET confirmed_at = ?, last_used_step = ? WHERE account_id = ? AND confirmed_at IS NULL")
                .bind(Timestamp::now().seconds())
                .bind(step_value(step))
                .bind(account.0)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 1;
        if !confirmed {
            return Ok(false);
        }
        replace_codes(&mut tx, account, recovery_digests).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Accepts time step `step` for an enabled account, unless it (or a later one) was
    /// already accepted. `true` means the code may be honoured.
    pub async fn claim_step(&self, account: AccountId, step: u64) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE two_factor SET last_used_step = ?1 WHERE account_id = ?2 AND confirmed_at IS NOT NULL \
             AND (last_used_step IS NULL OR last_used_step < ?1)",
        )
        .bind(step_value(step))
        .bind(account.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Marks an unused recovery code as used. `true` if it existed and was unused.
    pub async fn spend_recovery_code(&self, account: AccountId, digest: &[u8; 32]) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE recovery_codes SET used_at = ? WHERE account_id = ? AND code_digest = ? AND used_at IS NULL")
            .bind(Timestamp::now().seconds())
            .bind(account.0)
            .bind(digest.as_slice())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn remaining_recovery_codes(&self, account: AccountId) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM recovery_codes WHERE account_id = ? AND used_at IS NULL")
            .bind(account.0)
            .fetch_one(&self.pool)
            .await?)
    }

    /// Replaces every recovery code of an enabled account. `false` if 2FA is not enabled.
    pub async fn replace_recovery_codes(&self, account: AccountId, digests: &[[u8; 32]]) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let enabled: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM two_factor WHERE account_id = ? AND confirmed_at IS NOT NULL)")
                .bind(account.0)
                .fetch_one(&mut *tx)
                .await?;
        if !enabled {
            return Ok(false);
        }
        replace_codes(&mut tx, account, digests).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Removes the secret and every recovery code. `true` if anything was removed.
    pub async fn disable(&self, account: AccountId) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let removed = sqlx::query("DELETE FROM two_factor WHERE account_id = ?").bind(account.0).execute(&mut *tx).await?.rows_affected();
        sqlx::query("DELETE FROM recovery_codes WHERE account_id = ?").bind(account.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(removed > 0)
    }
}

/// Time steps are far below `i64::MAX` for the next few billion years.
fn step_value(step: u64) -> i64 {
    i64::try_from(step).unwrap_or(i64::MAX)
}

async fn replace_codes(tx: &mut sqlx::SqliteConnection, account: AccountId, digests: &[[u8; 32]]) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM recovery_codes WHERE account_id = ?").bind(account.0).execute(&mut *tx).await?;
    for digest in digests {
        sqlx::query("INSERT INTO recovery_codes (account_id, code_digest) VALUES (?, ?)")
            .bind(account.0)
            .bind(digest.as_slice())
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}
