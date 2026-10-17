//! Accounts linked to identities at external sign-in providers (migration
//! `0037_external_identities.sql`).
//!
//! Milestone 3 workstream O (external sign-in) owns this file.
//!
//! Which identity may be linked to which account is the service's decision; this store
//! keeps the two uniqueness rules (one account per identity, one identity per provider
//! and account) and makes the two operations that must not race atomic: registering an
//! account together with its identity, and unlinking an identity only while the account
//! keeps another way to sign in.

use rbit_core::{AccountId, Timestamp, User};
use sqlx::SqlitePool;

use super::accounts::{NewUser, insert_user};
use crate::StorageError;

/// A linked identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdentity {
    pub id: i64,
    pub account_id: AccountId,
    pub provider_id: String,
    pub subject: String,
    /// What the provider last reported (informational).
    pub email: Option<String>,
    /// The provider's login name or preferred username, last reported (informational).
    pub username: Option<String>,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

#[derive(Debug, sqlx::FromRow)]
struct IdentityRow {
    id: i64,
    account_id: i64,
    provider_id: String,
    subject: String,
    email: Option<String>,
    username: Option<String>,
    created_at: i64,
    last_used_at: Option<i64>,
}

impl From<IdentityRow> for ExternalIdentity {
    fn from(row: IdentityRow) -> Self {
        Self {
            id: row.id,
            account_id: AccountId(row.account_id),
            provider_id: row.provider_id,
            subject: row.subject,
            email: row.email,
            username: row.username,
            created_at: Timestamp(row.created_at),
            last_used_at: row.last_used_at.map(Timestamp),
        }
    }
}

/// An identity to link.
#[derive(Debug, Clone, Copy)]
pub struct NewExternalIdentity<'a> {
    pub provider_id: &'a str,
    pub subject: &'a str,
    pub email: Option<&'a str>,
    pub username: Option<&'a str>,
}

/// Why an identity could not be linked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkConflict {
    /// The identity is already linked to an account (possibly this one).
    IdentityTaken,
    /// The account already has an identity at this provider.
    ProviderTaken,
}

/// The result of [`ExternalIdentityStore::unlink`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unlinked {
    Removed,
    /// The account has no identity at that provider.
    NotFound,
    /// Refused: the account has no password and no other usable identity.
    LastMethod,
}

/// Columns of [`ExternalIdentity`], in order.
macro_rules! select_identity {
    ($clause:literal) => {
        concat!("SELECT id, account_id, provider_id, subject, email, username, created_at, last_used_at FROM external_identities ", $clause)
    };
}

#[derive(Debug, Clone)]
pub struct ExternalIdentityStore {
    pool: SqlitePool,
}

impl ExternalIdentityStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The identity `subject` at `provider`, if linked.
    pub async fn find(&self, provider: &str, subject: &str) -> Result<Option<ExternalIdentity>, StorageError> {
        let row: Option<IdentityRow> = sqlx::query_as(select_identity!("WHERE provider_id = ? AND subject = ?"))
            .bind(provider)
            .bind(subject)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(ExternalIdentity::from))
    }

    /// The account's identities, oldest first.
    pub async fn list_for_account(&self, account: AccountId) -> Result<Vec<ExternalIdentity>, StorageError> {
        let rows: Vec<IdentityRow> =
            sqlx::query_as(select_identity!("WHERE account_id = ? ORDER BY created_at, id")).bind(account.0).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(ExternalIdentity::from).collect())
    }

    /// Links `identity` to `account`.
    pub async fn link(
        &self,
        account: AccountId,
        identity: NewExternalIdentity<'_>,
        at: Timestamp,
    ) -> Result<Result<ExternalIdentity, LinkConflict>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let inserted = insert(&mut tx, account, identity, at).await?;
        if inserted.is_ok() {
            tx.commit().await?;
        }
        Ok(inserted)
    }

    /// Records a sign-in with the identity and what the provider reported this time.
    pub async fn touch(&self, id: i64, email: Option<&str>, username: Option<&str>, at: Timestamp) -> Result<(), StorageError> {
        sqlx::query("UPDATE external_identities SET email = ?, username = ?, last_used_at = ? WHERE id = ?")
            .bind(email)
            .bind(username)
            .bind(at.0)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Removes the account's identity at `provider` unless that would leave the account
    /// without a way to sign in: it must have a password, or another identity at one of
    /// `usable_providers` (the configured ones). Checked and deleted in one statement.
    pub async fn unlink(&self, account: AccountId, provider: &str, usable_providers: &[&str]) -> Result<Unlinked, StorageError> {
        // A JSON array keeps the statement static whatever the number of providers.
        let usable = serde_json::to_string(usable_providers).map_err(|error| StorageError::Corrupt(error.to_string()))?;
        let removed = sqlx::query(
            "DELETE FROM external_identities WHERE account_id = ?1 AND provider_id = ?2 AND ( \
                 EXISTS (SELECT 1 FROM user_logins WHERE account_id = ?1 AND password_hash <> '') \
                 OR EXISTS (SELECT 1 FROM external_identities WHERE account_id = ?1 AND provider_id <> ?2 \
                            AND provider_id IN (SELECT value FROM json_each(?3))))",
        )
        .bind(account.0)
        .bind(provider)
        .bind(usable)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if removed > 0 {
            return Ok(Unlinked::Removed);
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM external_identities WHERE account_id = ? AND provider_id = ?)")
            .bind(account.0)
            .bind(provider)
            .fetch_one(&self.pool)
            .await?;
        Ok(if exists { Unlinked::LastMethod } else { Unlinked::NotFound })
    }

    /// The user whose sign-in address is `email` **and verified**, if any.
    pub async fn account_with_verified_email(&self, email: &str) -> Result<Option<AccountId>, StorageError> {
        let id: Option<i64> = sqlx::query_scalar("SELECT account_id FROM user_logins WHERE email = ? AND email_verified_at IS NOT NULL")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(id.map(AccountId))
    }

    /// Whether the user has a password (accounts registered through a provider start
    /// without one; their stored hash is empty and never verifies).
    pub async fn has_password(&self, account: AccountId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_logins WHERE account_id = ? AND password_hash <> '')")
            .bind(account.0)
            .fetch_one(&self.pool)
            .await?)
    }

    /// Creates a user (normally with an empty password hash) linked to `identity`, all or
    /// nothing; `email_verified` marks the address verified. The same two inserts as
    /// [`super::AccountStore::create_user`], plus the identity, in one transaction.
    pub async fn register(
        &self,
        new: NewUser,
        email_verified: bool,
        identity: NewExternalIdentity<'_>,
        registration_open: bool,
    ) -> Result<Result<User, LinkConflict>, StorageError> {
        let now = Timestamp::now();
        let mut tx = self.pool.begin().await?;
        let user = insert_user(&mut tx, new, Some(registration_open), email_verified).await?;
        if let Err(conflict) = insert(&mut tx, user.id(), identity, now).await? {
            return Ok(Err(conflict));
        }
        tx.commit().await?;
        Ok(Ok(user))
    }
}

/// Inserts an identity inside `tx`, reporting which uniqueness rule refused it.
async fn insert(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account: AccountId,
    identity: NewExternalIdentity<'_>,
    at: Timestamp,
) -> Result<Result<ExternalIdentity, LinkConflict>, StorageError> {
    let inserted: Result<IdentityRow, sqlx::Error> = sqlx::query_as(concat!(
        "INSERT INTO external_identities (account_id, provider_id, subject, email, username, created_at, last_used_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) ",
        "RETURNING id, account_id, provider_id, subject, email, username, created_at, last_used_at"
    ))
    .bind(account.0)
    .bind(identity.provider_id)
    .bind(identity.subject)
    .bind(identity.email)
    .bind(identity.username)
    .bind(at.0)
    .bind(at.0)
    .fetch_one(&mut **tx)
    .await;
    match inserted {
        Ok(row) => Ok(Ok(row.into())),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM external_identities WHERE provider_id = ? AND subject = ?)")
                .bind(identity.provider_id)
                .bind(identity.subject)
                .fetch_one(&mut **tx)
                .await?;
            Ok(Err(if taken { LinkConflict::IdentityTaken } else { LinkConflict::ProviderTaken }))
        }
        Err(error) => Err(error.into()),
    }
}
