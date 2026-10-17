//! Email notifications: each user's email preference and the outbox that makes sure a
//! notification is mailed at most once (migration `0034_notification_email.sql`).
//!
//! Milestone 3 workstream N (email notifications) owns this file.
//!
//! Who receives what is the service's decision; this store only records preferences and
//! claims dedup keys.

use rbit_core::{AccountId, IssueId, Timestamp, config::EmailPreference};
use sqlx::SqlitePool;

use crate::StorageError;

#[derive(Debug, Clone)]
pub struct NotificationEmailStore {
    pool: SqlitePool,
}

impl NotificationEmailStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The preference `account` chose, or `None` if it never chose one.
    pub async fn preference(&self, account: AccountId) -> Result<Option<EmailPreference>, StorageError> {
        let stored: Option<String> = sqlx::query_scalar("SELECT email FROM notification_email_settings WHERE account_id = ?")
            .bind(account.0)
            .fetch_optional(&self.pool)
            .await?;
        stored
            .map(|value| EmailPreference::parse(&value).ok_or_else(|| StorageError::Corrupt(format!("email preference: {value}"))))
            .transpose()
    }

    /// Stores (or replaces) the preference of `account`.
    pub async fn set_preference(&self, account: AccountId, preference: EmailPreference, at: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO notification_email_settings (account_id, email, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT (account_id) DO UPDATE SET email = excluded.email, updated_at = excluded.updated_at",
        )
        .bind(account.0)
        .bind(preference.as_str())
        .bind(at.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Claims the email about `key` on `issue` for `account`: `true` the first time,
    /// `false` when it was claimed before (the email must not be sent again).
    pub async fn claim(&self, account: AccountId, issue: IssueId, key: &str, at: Timestamp) -> Result<bool, StorageError> {
        let inserted = sqlx::query(
            "INSERT INTO notification_emails (account_id, issue_id, dedup_key, created_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT (account_id, issue_id, dedup_key) DO NOTHING",
        )
        .bind(account.0)
        .bind(issue.0)
        .bind(key)
        .bind(at.0)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(inserted > 0)
    }

    /// Deletes outbox rows created before `before`; returns how many.
    pub async fn prune(&self, before: Timestamp) -> Result<u64, StorageError> {
        Ok(sqlx::query("DELETE FROM notification_emails WHERE created_at < ?").bind(before.0).execute(&self.pool).await?.rows_affected())
    }
}
