//! TOTP secrets, passkeys and hashed recovery codes.
//!
//! Milestone 1 workstream S owns this file. Keep row types here rather than in `rows.rs`.
//!
//! Every state change that must not race (confirming enrolment, accepting a time step,
//! spending a recovery code, advancing a passkey's counter) is a single guarded
//! statement, so two concurrent requests can never both succeed with the same code.
//!
//! An account has two-factor authentication while it has a confirmed authenticator app
//! or at least one passkey. Recovery codes belong to the account and are removed with its
//! last factor.

use super::pool::Pool;
use rbit_core::{AccountId, Timestamp};

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

/// A registered passkey or security key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasskeyRecord {
    pub id: i64,
    pub credential_id: Vec<u8>,
    /// The COSE key as registered.
    pub public_key: Vec<u8>,
    pub algorithm: i64,
    pub sign_count: u32,
    pub transports: Vec<String>,
    pub name: String,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

/// A passkey to register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewPasskey<'a> {
    pub credential_id: &'a [u8],
    pub public_key: &'a [u8],
    pub algorithm: i64,
    pub sign_count: u32,
    pub transports: &'a [String],
    pub name: &'a str,
}

/// What [`TwoFactorStore::add_passkey`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasskeyAdded {
    /// Added to an account that already had two-factor authentication.
    Added,
    /// Added as the account's first factor: the recovery codes given were stored.
    FirstFactor,
    /// This credential is already registered (to this or another account).
    Duplicate,
}

#[derive(Debug, sqlx::FromRow)]
struct PasskeyRow {
    id: i64,
    credential_id: Vec<u8>,
    public_key: Vec<u8>,
    algorithm: i64,
    sign_count: i64,
    transports: String,
    name: String,
    created_at: i64,
    last_used_at: Option<i64>,
}

impl From<PasskeyRow> for PasskeyRecord {
    fn from(row: PasskeyRow) -> Self {
        Self {
            id: row.id,
            credential_id: row.credential_id,
            public_key: row.public_key,
            algorithm: row.algorithm,
            sign_count: u32::try_from(row.sign_count).unwrap_or(u32::MAX),
            transports: row.transports.split(',').filter(|t| !t.is_empty()).map(str::to_owned).collect(),
            name: row.name,
            created_at: Timestamp(row.created_at),
            last_used_at: row.last_used_at.map(Timestamp),
        }
    }
}

/// Statements are assembled with `concat!` so they stay `'static` (checked by `sqlx`).
macro_rules! passkey_columns {
    () => {
        "id, credential_id, public_key, algorithm, sign_count, transports, name, created_at, last_used_at"
    };
}

/// Whether account `?1` has any confirmed factor, as an SQL expression.
macro_rules! has_factor {
    () => {
        "(EXISTS (SELECT 1 FROM two_factor WHERE account_id = ?1 AND confirmed_at IS NOT NULL) \
     OR EXISTS (SELECT 1 FROM passkeys WHERE account_id = ?1))"
    };
}

#[derive(Debug, Clone)]
pub struct TwoFactorStore {
    pool: Pool,
}

impl TwoFactorStore {
    pub(crate) fn new(pool: Pool) -> Self {
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

    /// Whether the account has two-factor authentication: a confirmed authenticator app or
    /// a passkey.
    pub async fn is_enabled(&self, account: AccountId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar(concat!("SELECT ", has_factor!())).bind(account.0).fetch_one(&self.pool).await?)
    }

    /// Stores a pending secret, replacing an earlier pending one. Returns `false` (and
    /// changes nothing) when the authenticator app is already enabled.
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

    /// Enables a pending enrolment and records the confirming code's step. If the account
    /// had no other factor, `recovery_digests` become its recovery codes (`Some(true)`);
    /// otherwise its codes are kept (`Some(false)`). `None` if there was no pending
    /// enrolment.
    pub async fn confirm(
        &self,
        account: AccountId,
        secret: &[u8],
        step: u64,
        recovery_digests: &[[u8; 32]],
    ) -> Result<Option<bool>, StorageError> {
        let mut tx = self.pool.begin_immediate().await?;
        let first_factor = !has_factor(&mut tx, account).await?;
        let confirmed = sqlx::query(
            "UPDATE two_factor SET confirmed_at = ?, last_used_step = ? \
             WHERE account_id = ? AND secret = ? AND confirmed_at IS NULL",
        )
        .bind(Timestamp::now().seconds())
        .bind(step_value(step))
        .bind(account.0)
        .bind(secret)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if !confirmed {
            return Ok(None);
        }
        if first_factor {
            replace_codes(&mut tx, account, recovery_digests).await?;
        }
        tx.commit().await?;
        Ok(Some(first_factor))
    }

    /// Accepts time step `step` for an enabled account, unless it (or a later one) was
    /// already accepted. `true` means the code may be honoured.
    pub async fn claim_step(&self, account: AccountId, secret: &[u8], step: u64) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE two_factor SET last_used_step = ?1 WHERE account_id = ?2 AND secret = ?3 AND confirmed_at IS NOT NULL \
             AND (last_used_step IS NULL OR last_used_step < ?1)",
        )
        .bind(step_value(step))
        .bind(account.0)
        .bind(secret)
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
        let mut tx = self.pool.begin_immediate().await?;
        if !has_factor(&mut tx, account).await? {
            return Ok(false);
        }
        replace_codes(&mut tx, account, digests).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Discards only an unconfirmed authenticator enrolment. Confirmation racing this
    /// operation must never turn cancellation into removal of an enabled factor.
    pub async fn cancel_enrolment(&self, account: AccountId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM two_factor WHERE account_id = ? AND confirmed_at IS NULL").bind(account.0).execute(&self.pool).await?;
        Ok(())
    }

    /// Removes the authenticator app (confirmed or pending), and the recovery codes too if
    /// that leaves no factor. `true` if anything was removed.
    pub async fn disable(&self, account: AccountId) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let removed = sqlx::query("DELETE FROM two_factor WHERE account_id = ?").bind(account.0).execute(&mut *tx).await?.rows_affected();
        drop_codes_without_factor(&mut tx, account).await?;
        tx.commit().await?;
        Ok(removed > 0)
    }

    // ------------------------------------------------------------------ passkeys

    /// The account's passkeys, oldest first.
    pub async fn passkeys(&self, account: AccountId) -> Result<Vec<PasskeyRecord>, StorageError> {
        let rows: Vec<PasskeyRow> =
            sqlx::query_as(concat!("SELECT ", passkey_columns!(), " FROM passkeys WHERE account_id = ? ORDER BY id"))
                .bind(account.0)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(PasskeyRecord::from).collect())
    }

    /// The account's passkey with credential ID `credential_id`.
    pub async fn find_passkey(&self, account: AccountId, credential_id: &[u8]) -> Result<Option<PasskeyRecord>, StorageError> {
        let row: Option<PasskeyRow> =
            sqlx::query_as(concat!("SELECT ", passkey_columns!(), " FROM passkeys WHERE account_id = ? AND credential_id = ?"))
                .bind(account.0)
                .bind(credential_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(PasskeyRecord::from))
    }

    /// Registers a passkey. If it is the account's first factor, `recovery_digests` become
    /// its recovery codes.
    pub async fn add_passkey(
        &self,
        account: AccountId,
        passkey: &NewPasskey<'_>,
        recovery_digests: &[[u8; 32]],
    ) -> Result<PasskeyAdded, StorageError> {
        let mut tx = self.pool.begin_immediate().await?;
        let first_factor = !has_factor(&mut tx, account).await?;
        let inserted = sqlx::query(
            "INSERT INTO passkeys (account_id, credential_id, public_key, algorithm, sign_count, transports, name, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (credential_id) DO NOTHING",
        )
        .bind(account.0)
        .bind(passkey.credential_id)
        .bind(passkey.public_key)
        .bind(passkey.algorithm)
        .bind(i64::from(passkey.sign_count))
        .bind(passkey.transports.join(","))
        .bind(passkey.name)
        .bind(Timestamp::now().seconds())
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if !inserted {
            return Ok(PasskeyAdded::Duplicate);
        }
        if first_factor {
            replace_codes(&mut tx, account, recovery_digests).await?;
        }
        tx.commit().await?;
        Ok(if first_factor { PasskeyAdded::FirstFactor } else { PasskeyAdded::Added })
    }

    /// Records a sign-in with passkey `id`: its counter moves from `seen_count` (the value
    /// the signature was checked against) to `new_count`. `false` if another sign-in got
    /// there first, so one response can't be honoured twice.
    pub async fn use_passkey(&self, account: AccountId, id: i64, seen_count: u32, new_count: u32) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE passkeys SET sign_count = ?, last_used_at = ? WHERE id = ? AND account_id = ? AND sign_count = ?")
            .bind(i64::from(new_count))
            .bind(Timestamp::now().seconds())
            .bind(id)
            .bind(account.0)
            .bind(i64::from(seen_count))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Removes passkey `id`, and the recovery codes too if that leaves no factor. `true` if
    /// it existed.
    pub async fn remove_passkey(&self, account: AccountId, id: i64) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let removed = sqlx::query("DELETE FROM passkeys WHERE id = ? AND account_id = ?")
            .bind(id)
            .bind(account.0)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        drop_codes_without_factor(&mut tx, account).await?;
        tx.commit().await?;
        Ok(removed > 0)
    }
}

/// Time steps are far below `i64::MAX` for the next few billion years.
fn step_value(step: u64) -> i64 {
    i64::try_from(step).unwrap_or(i64::MAX)
}

async fn has_factor(tx: &mut super::pool::Conn, account: AccountId) -> Result<bool, StorageError> {
    Ok(sqlx::query_scalar(concat!("SELECT ", has_factor!())).bind(account.0).fetch_one(&mut *tx).await?)
}

async fn drop_codes_without_factor(tx: &mut super::pool::Conn, account: AccountId) -> Result<(), StorageError> {
    sqlx::query(concat!("DELETE FROM recovery_codes WHERE account_id = ?1 AND NOT ", has_factor!()))
        .bind(account.0)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

async fn replace_codes(tx: &mut super::pool::Conn, account: AccountId, digests: &[[u8; 32]]) -> Result<(), StorageError> {
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
