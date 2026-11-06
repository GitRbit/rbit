//! User interface choices: theme in `user_preferences`, language on the already-read
//! `user_logins` row so authenticated page requests need no extra preference lookup.

use super::pool::Pool;
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::AccountLocale;

use crate::StorageError;

#[derive(Debug, Clone)]
pub struct PreferenceStore {
    pool: Pool,
}

impl PreferenceStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// The account's stored theme slug, or `None` if it never chose one.
    pub async fn theme(&self, account: AccountId) -> Result<Option<String>, StorageError> {
        let theme: Option<Option<String>> = sqlx::query_scalar("SELECT theme FROM user_preferences WHERE account_id = ?")
            .bind(account.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(theme.flatten())
    }

    /// Stores (or replaces) the account's theme slug.
    pub async fn set_theme(&self, account: AccountId, theme: &str) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO user_preferences (account_id, theme, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT (account_id) DO UPDATE SET theme = excluded.theme, updated_at = excluded.updated_at",
        )
        .bind(account.0)
        .bind(theme)
        .bind(Timestamp::now().seconds())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Canonical language or browser/default, without allocating a request String.
    pub async fn locale(&self, account: AccountId) -> Result<Option<AccountLocale>, StorageError> {
        let code: Option<i64> =
            sqlx::query_scalar("SELECT CASE locale WHEN 'en' THEN 1 WHEN 'es' THEN 2 ELSE 0 END FROM user_logins WHERE account_id = ?")
                .bind(account.0)
                .fetch_optional(&self.pool)
                .await?;
        Ok(locale_from_code(code.unwrap_or_default()))
    }

    /// Setting/resetting language leaves the independently stored theme untouched.
    pub async fn set_locale(&self, account: AccountId, locale: Option<AccountLocale>) -> Result<bool, StorageError> {
        // Preference-only: never touch password, authentication epoch or pending barriers.
        let result = sqlx::query("UPDATE user_logins SET locale = ? WHERE account_id = ?")
            .bind(locale.map(AccountLocale::tag))
            .bind(account.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }
}

/// A small SQL projection avoids allocating the stored two-byte tag on session reads.
pub(crate) const fn locale_from_code(code: i64) -> Option<AccountLocale> {
    match code {
        1 => Some(AccountLocale::English),
        2 => Some(AccountLocale::Spanish),
        _ => None,
    }
}
