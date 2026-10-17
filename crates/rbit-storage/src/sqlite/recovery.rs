//! Password-reset and email-verification tokens (digests only), and the verification
//! status of each user's address.
//!
//! Milestone 1 workstream E owns this file. Keep row types here rather than in `rows.rs`.
//!
//! A token is only ever honoured while it is unused, unexpired *and* still addressed to the
//! account's current email: changing the address silently invalidates every link sent to
//! the old one. [`RecoveryStore::consume`] checks all of that and marks the token used in a
//! single statement, so two concurrent requests can never both redeem it.

use std::collections::HashSet;

use rbit_core::{AccountId, AccountName, Email, Timestamp};
use sqlx::SqlitePool;

use crate::StorageError;

/// What a token lets its holder do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPurpose {
    /// Choose a new password without knowing the old one.
    Reset,
    /// Prove control of the address the token was sent to.
    Verify,
}

impl TokenPurpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reset => "reset",
            Self::Verify => "verify",
        }
    }
}

/// A token to store. `digest` is the SHA-256 of the secret that went into the link.
#[derive(Debug, Clone)]
pub struct NewEmailToken {
    pub digest: [u8; 32],
    pub account_id: AccountId,
    pub purpose: TokenPurpose,
    /// The address the link is sent to.
    pub email: Email,
    pub expires_at: Timestamp,
}

/// A token that is currently honoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailTokenRecord {
    pub account_id: AccountId,
    pub email: Email,
    pub expires_at: Timestamp,
}

#[derive(Debug, sqlx::FromRow)]
struct TokenRow {
    account_id: i64,
    email: String,
    expires_at: i64,
}

impl TryFrom<TokenRow> for EmailTokenRecord {
    type Error = StorageError;

    fn try_from(row: TokenRow) -> Result<Self, Self::Error> {
        Ok(Self {
            account_id: AccountId(row.account_id),
            email: row.email.parse().map_err(|e| StorageError::Corrupt(format!("token email: {e}")))?,
            expires_at: Timestamp(row.expires_at),
        })
    }
}

/// The parts of a user that recovery flows need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryAccount {
    pub id: AccountId,
    pub name: AccountName,
    pub email: Email,
    pub suspended: bool,
    pub email_verified_at: Option<Timestamp>,
}

#[derive(Debug, sqlx::FromRow)]
struct AccountRow {
    id: i64,
    name: String,
    email: String,
    suspended_at: Option<i64>,
    email_verified_at: Option<i64>,
}

impl TryFrom<AccountRow> for RecoveryAccount {
    type Error = StorageError;

    fn try_from(row: AccountRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: AccountId(row.id),
            name: row.name.parse().map_err(|e| StorageError::Corrupt(format!("account name: {e}")))?,
            email: row.email.parse().map_err(|e| StorageError::Corrupt(format!("email: {e}")))?,
            suspended: row.suspended_at.is_some(),
            email_verified_at: row.email_verified_at.map(Timestamp),
        })
    }
}

/// Builds a static `SELECT` over users for recovery with the given trailing clause.
macro_rules! select_account {
    ($clause:literal) => {
        concat!(
            "SELECT a.id, a.name, l.email, l.suspended_at, l.email_verified_at \
             FROM accounts a JOIN user_logins l ON l.account_id = a.id ",
            $clause
        )
    };
}

/// The conditions under which a token is honoured (`?1` = now, `?2` = digest, `?3` = purpose).
macro_rules! honoured {
    () => {
        "t.digest = ?2 AND t.purpose = ?3 AND t.used_at IS NULL AND t.expires_at > ?1 \
         AND t.email = (SELECT l.email FROM user_logins l WHERE l.account_id = t.account_id)"
    };
}

#[derive(Debug, Clone)]
pub struct RecoveryStore {
    pool: SqlitePool,
}

impl RecoveryStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Stores a token. Also sweeps tokens that expired, so the table stays small.
    pub async fn insert(&self, token: &NewEmailToken) -> Result<(), StorageError> {
        let now = Timestamp::now().seconds();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM email_tokens WHERE expires_at <= ?").bind(now).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO email_tokens (digest, account_id, purpose, email, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(token.digest.as_slice())
            .bind(token.account_id.0)
            .bind(token.purpose.as_str())
            .bind(token.email.as_str())
            .bind(now)
            .bind(token.expires_at.seconds())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The token, if it would be honoured at `now`. Does not use it up.
    pub async fn find_valid(
        &self,
        digest: &[u8; 32],
        purpose: TokenPurpose,
        now: Timestamp,
    ) -> Result<Option<EmailTokenRecord>, StorageError> {
        let row: Option<TokenRow> =
            sqlx::query_as(concat!("SELECT t.account_id, t.email, t.expires_at FROM email_tokens t WHERE ", honoured!()))
                .bind(now.seconds())
                .bind(digest.as_slice())
                .bind(purpose.as_str())
                .fetch_optional(&self.pool)
                .await?;
        row.map(EmailTokenRecord::try_from).transpose()
    }

    /// Marks the token used and returns it, if it was honoured at `now`. Atomic: of two
    /// concurrent calls with the same digest, at most one gets `Some`.
    pub async fn consume(
        &self,
        digest: &[u8; 32],
        purpose: TokenPurpose,
        now: Timestamp,
    ) -> Result<Option<EmailTokenRecord>, StorageError> {
        let row: Option<TokenRow> = sqlx::query_as(concat!(
            "UPDATE email_tokens AS t SET used_at = ?1 WHERE ",
            honoured!(),
            " RETURNING account_id, email, expires_at"
        ))
        .bind(now.seconds())
        .bind(digest.as_slice())
        .bind(purpose.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(EmailTokenRecord::try_from).transpose()
    }

    /// Redeems a reset link, replaces the password, verifies its address and revokes
    /// every other reset link in one transaction. Concurrent links cannot both win,
    /// and email changes or suspension before the write invalidate the reset.
    pub async fn reset_password(
        &self,
        digest: &[u8; 32],
        password_hash: &str,
        now: Timestamp,
    ) -> Result<Option<EmailTokenRecord>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let row: Option<TokenRow> = sqlx::query_as(concat!(
            "UPDATE email_tokens AS t SET used_at = ?1 WHERE ",
            honoured!(),
            " AND EXISTS (SELECT 1 FROM user_logins l WHERE l.account_id = t.account_id AND l.suspended_at IS NULL) \
             RETURNING account_id, email, expires_at"
        ))
        .bind(now.seconds())
        .bind(digest.as_slice())
        .bind(TokenPurpose::Reset.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let record = EmailTokenRecord::try_from(row)?;
        sqlx::query("UPDATE user_logins SET password_hash = ?, email_verified_at = ? WHERE account_id = ?")
            .bind(password_hash)
            .bind(now.seconds())
            .bind(record.account_id.0)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE email_tokens SET used_at = ? WHERE account_id = ? AND purpose = 'reset' AND used_at IS NULL")
            .bind(now.seconds())
            .bind(record.account_id.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(record))
    }

    /// Uses up every outstanding token of `purpose` for the account. Returns how many.
    pub async fn revoke_all(&self, account_id: AccountId, purpose: TokenPurpose) -> Result<u64, StorageError> {
        let result = sqlx::query("UPDATE email_tokens SET used_at = ? WHERE account_id = ? AND purpose = ? AND used_at IS NULL")
            .bind(Timestamp::now().seconds())
            .bind(account_id.0)
            .bind(purpose.as_str())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// The user with this sign-in address (case-insensitive), if any.
    pub async fn find_account_by_email(&self, email: &Email) -> Result<Option<RecoveryAccount>, StorageError> {
        let row: Option<AccountRow> =
            sqlx::query_as(select_account!("WHERE l.email = ?")).bind(email.as_str()).fetch_optional(&self.pool).await?;
        row.map(RecoveryAccount::try_from).transpose()
    }

    pub async fn find_account(&self, id: AccountId) -> Result<Option<RecoveryAccount>, StorageError> {
        let row: Option<AccountRow> = sqlx::query_as(select_account!("WHERE a.id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(RecoveryAccount::try_from).transpose()
    }

    /// Records that the account's address was verified, provided it is still `email`.
    /// Returns whether the account was updated.
    pub async fn mark_verified(&self, account_id: AccountId, email: &Email, at: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE user_logins SET email_verified_at = ? WHERE account_id = ? AND email = ?")
            .bind(at.seconds())
            .bind(account_id.0)
            .bind(email.as_str())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Which of `ids` have a verified address.
    pub async fn verified_among(&self, ids: &[AccountId]) -> Result<HashSet<AccountId>, StorageError> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        // A JSON array keeps the statement static whatever the number of ids.
        let list = format!("[{}]", ids.iter().map(|id| id.0.to_string()).collect::<Vec<_>>().join(","));
        let verified: Vec<i64> = sqlx::query_scalar(
            "SELECT account_id FROM user_logins \
             WHERE email_verified_at IS NOT NULL AND account_id IN (SELECT value FROM json_each(?))",
        )
        .bind(list)
        .fetch_all(&self.pool)
        .await?;
        Ok(verified.into_iter().map(AccountId).collect())
    }
}
