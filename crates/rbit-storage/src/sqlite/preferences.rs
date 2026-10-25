//! Per-user interface preferences (`user_preferences`).
//!
//! Frontend workstream T owns this file.

use super::pool::Pool;
use rbit_core::{AccountId, Timestamp};

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
}
