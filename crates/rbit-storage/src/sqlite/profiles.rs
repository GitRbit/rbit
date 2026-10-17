//! User profile fields and account credentials (display name, bio, website, email, password).
//!
//! Workstream D owns this file. Keep row types here rather than in `rows.rs`.

use rbit_core::{AccountId, Email, Timestamp};
use sqlx::SqlitePool;

use crate::StorageError;

/// Editable profile fields of an account. `None` means "not set".
///
/// `display_name` and `bio` live on `accounts` (shared with organisations); `website` and
/// `location` live in `account_profiles`.
#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct AccountProfile {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    pool: SqlitePool,
}

impl ProfileStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The account's profile, or `None` if the account does not exist.
    pub async fn find(&self, account_id: AccountId) -> Result<Option<AccountProfile>, StorageError> {
        Ok(sqlx::query_as(
            "SELECT a.display_name, a.bio, p.website, p.location \
             FROM accounts a LEFT JOIN account_profiles p ON p.account_id = a.id WHERE a.id = ?",
        )
        .bind(account_id.0)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Replaces every profile field atomically.
    pub async fn update(&self, account_id: AccountId, profile: &AccountProfile) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE accounts SET display_name = ?, bio = ?, updated_at = ? WHERE id = ?")
            .bind(profile.display_name.as_deref())
            .bind(profile.bio.as_deref())
            .bind(Timestamp::now().seconds())
            .bind(account_id.0)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO account_profiles (account_id, website, location) VALUES (?, ?, ?) \
             ON CONFLICT (account_id) DO UPDATE SET website = excluded.website, location = excluded.location",
        )
        .bind(account_id.0)
        .bind(profile.website.as_deref())
        .bind(profile.location.as_deref())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The stored Argon2id hash of a user's password, or `None` if the user does not exist.
    pub async fn password_hash(&self, account_id: AccountId) -> Result<Option<String>, StorageError> {
        Ok(sqlx::query_scalar("SELECT password_hash FROM user_logins WHERE account_id = ?")
            .bind(account_id.0)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// [`StorageError::Conflict`] (`"email"`) if another user already uses `email`.
    ///
    /// A different address starts out unverified; re-entering the same one (in any letter
    /// case) keeps its verification.
    pub async fn set_email(&self, account_id: AccountId, email: &Email) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE user_logins SET email = ?1, \
             email_verified_at = CASE WHEN email = ?1 THEN email_verified_at ELSE NULL END WHERE account_id = ?2",
        )
        .bind(email.as_str())
        .bind(account_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "email"))?;
        Ok(())
    }

    /// Stores a new password hash. The password must already be hashed.
    pub async fn set_password_hash(&self, account_id: AccountId, password_hash: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE user_logins SET password_hash = ? WHERE account_id = ?")
            .bind(password_hash)
            .bind(account_id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
