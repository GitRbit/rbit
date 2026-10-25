//! Email notifications: each user's email preference and the outbox that makes sure a
//! notification is mailed at most once (migration `0034_notification_email.sql`), and how
//! they are delivered: one by one or in digests (`0047_email_digests.sql`).
//!
//! Milestone 3 workstream N (email notifications) owns this file.
//!
//! Who receives what is the service's decision; this store only records preferences and
//! claims dedup keys.

use std::collections::{BTreeSet, HashSet};

use super::pool::Pool;
use rbit_core::{
    AccountId, IssueId, Timestamp, User,
    config::{EmailDelivery, EmailPreference},
};

use super::{
    email_digests::{parse_delivery, parse_hour},
    notifications::NOTIFICATION_BATCH_SIZE,
    rows::UserRow,
};
use crate::StorageError;

/// An active user whose current address is verified, with their email preference.
#[derive(Debug, Clone)]
pub struct EmailRecipient {
    pub user: User,
    pub preference: Option<EmailPreference>,
    /// How their notification emails are delivered (immediate unless they chose).
    pub delivery: EmailDelivery,
}

/// How a user's notification emails are delivered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeliverySettings {
    pub delivery: EmailDelivery,
    /// The hour (UTC) of their digests; `None` follows the instance default.
    pub digest_hour: Option<u8>,
}

#[derive(Debug, sqlx::FromRow)]
struct RecipientRow {
    #[sqlx(flatten)]
    user: UserRow,
    preference: Option<String>,
    delivery: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct DeliveryRow {
    delivery: String,
    digest_hour: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NotificationEmailStore {
    pool: Pool,
}

impl NotificationEmailStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Fresh account, address verification and preference data in one read per bounded
    /// batch. Suspended, unverified and unknown users are omitted; IDs are deduplicated.
    pub async fn recipients(&self, accounts: &[AccountId]) -> Result<Vec<EmailRecipient>, StorageError> {
        let accounts: Vec<_> = accounts.iter().map(|id| id.0).collect::<BTreeSet<_>>().into_iter().collect();
        let mut recipients = Vec::with_capacity(accounts.len());
        for batch in accounts.chunks(NOTIFICATION_BATCH_SIZE) {
            let ids = serde_json::to_string(batch).map_err(|error| StorageError::Corrupt(format!("email recipients: {error}")))?;
            let rows: Vec<RecipientRow> = sqlx::query_as(
                "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, l.email, l.is_admin, '' AS password_hash, \
                 s.email AS preference, s.delivery FROM accounts a JOIN user_logins l ON l.account_id = a.id \
                 LEFT JOIN notification_email_settings s ON s.account_id = a.id \
                 WHERE a.id IN (SELECT value FROM json_each(?)) AND l.suspended_at IS NULL AND l.email_verified_at IS NOT NULL \
                 ORDER BY a.id",
            )
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
            for row in rows {
                let preference = row
                    .preference
                    .map(|value| EmailPreference::parse(&value).ok_or_else(|| StorageError::Corrupt(format!("email preference: {value}"))))
                    .transpose()?;
                let delivery = parse_delivery(row.delivery.as_deref())?;
                recipients.push(EmailRecipient { user: row.user.into_user_and_hash()?.0, preference, delivery });
            }
        }
        Ok(recipients)
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

    /// How `account`'s notification emails are delivered (the defaults if it never chose).
    pub async fn delivery(&self, account: AccountId) -> Result<DeliverySettings, StorageError> {
        let row: Option<DeliveryRow> = sqlx::query_as("SELECT delivery, digest_hour FROM notification_email_settings WHERE account_id = ?")
            .bind(account.0)
            .fetch_optional(&self.pool)
            .await?;
        row.map_or(Ok(DeliverySettings::default()), |row| {
            Ok(DeliverySettings { delivery: parse_delivery(Some(&row.delivery))?, digest_hour: parse_hour(row.digest_hour)? })
        })
    }

    /// Stores (or replaces) the preference and the delivery settings of `account` together.
    pub async fn save(
        &self,
        account: AccountId,
        preference: EmailPreference,
        delivery: DeliverySettings,
        at: Timestamp,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO notification_email_settings (account_id, email, delivery, digest_hour, updated_at) VALUES (?, ?, ?, ?, ?)              ON CONFLICT (account_id) DO UPDATE SET email = excluded.email, delivery = excluded.delivery,              digest_hour = excluded.digest_hour, updated_at = excluded.updated_at",
        )
        .bind(account.0)
        .bind(preference.as_str())
        .bind(delivery.delivery.as_str())
        .bind(delivery.digest_hour.map(i64::from))
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

    /// Claims one activity for many recipients, returning only the first-time winners.
    /// Every statement commits at most [`NOTIFICATION_BATCH_SIZE`] claims; concurrent
    /// fan-outs cannot both send the same recipient/activity pair.
    pub async fn claim_many(
        &self,
        accounts: &[AccountId],
        issue: IssueId,
        key: &str,
        at: Timestamp,
    ) -> Result<HashSet<AccountId>, StorageError> {
        let accounts: Vec<_> = accounts.iter().map(|id| id.0).collect::<BTreeSet<_>>().into_iter().collect();
        let mut claimed = HashSet::with_capacity(accounts.len());
        for batch in accounts.chunks(NOTIFICATION_BATCH_SIZE) {
            let ids = serde_json::to_string(batch).map_err(|error| StorageError::Corrupt(format!("email recipients: {error}")))?;
            let inserted: Vec<i64> = sqlx::query_scalar(
                "INSERT INTO notification_emails (account_id, issue_id, dedup_key, created_at) \
                 SELECT value, ?1, ?2, ?3 FROM json_each(?4) WHERE 1 \
                 ON CONFLICT (account_id, issue_id, dedup_key) DO NOTHING RETURNING account_id",
            )
            .bind(issue.0)
            .bind(key)
            .bind(at.0)
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
            claimed.extend(inserted.into_iter().map(AccountId));
        }
        Ok(claimed)
    }

    /// Deletes outbox rows created before `before`; returns how many.
    pub async fn prune(&self, before: Timestamp) -> Result<u64, StorageError> {
        Ok(sqlx::query("DELETE FROM notification_emails WHERE created_at < ?").bind(before.0).execute(&self.pool).await?.rows_affected())
    }
}
