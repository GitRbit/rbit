//! Email digests: notifications waiting for a user's daily or weekly digest, and the
//! digests claimed for each period (migration `0047_email_digests.sql`).
//!
//! Who receives a digest and what it says is the service's decision; this store queues
//! items and makes sending idempotent. [`EmailDigestStore::claim`] inserts the one digest
//! row a user may have per period (`UNIQUE (account_id, period)`) and attaches the user's
//! pending items to it in the same transaction, so a period is claimed at most once, even
//! by concurrent or restarted senders. A claimed digest is never claimed again: sending is
//! at most once.

use rbit_core::{AccountId, IssueId, IssueKind, IssueNumber, RepoId, Timestamp, config::EmailDelivery};
use sqlx::SqlitePool;

use super::notifications::NotificationReason;
use crate::StorageError;

/// A notification that waits for a digest instead of being emailed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDigestItem {
    pub account: AccountId,
    pub repo: RepoId,
    pub issue: IssueId,
    pub reason: NotificationReason,
    /// One sentence, e.g. `bob commented on issue #3.`
    pub headline: String,
    /// An excerpt of the text, already shortened, or empty.
    pub excerpt: String,
    /// The fragment of the thread page to link to.
    pub anchor: Option<String>,
    pub created_at: Timestamp,
}

/// A user with notifications waiting for a digest, and their digest settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigestCandidate {
    pub account: AccountId,
    /// `Immediate` for users who switched back after notifications were queued.
    pub delivery: EmailDelivery,
    /// `None` follows the instance's default digest hour.
    pub digest_hour: Option<u8>,
}

/// A digest claimed for one user and period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimedDigest {
    pub id: i64,
    pub account: AccountId,
    pub period: Timestamp,
    /// The items attached to it.
    pub items: u64,
}

/// The outcome of [`EmailDigestStore::claim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// The period was claimed before (by this or another sender): nothing to do.
    Taken,
    /// Nothing queued before the period is left to send; no digest is recorded.
    Nothing {
        /// Items deleted because their thread was read in the app.
        skipped_read: u64,
    },
    /// The digest to send now.
    Claimed {
        digest: ClaimedDigest,
        /// Items left out because their thread was read in the app.
        skipped_read: u64,
    },
}

/// One notification of a claimed digest, with the current names of its thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestItem {
    pub repo: RepoId,
    pub issue: IssueId,
    pub reason: NotificationReason,
    pub headline: String,
    pub excerpt: String,
    pub anchor: Option<String>,
    pub created_at: Timestamp,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub title: String,
    pub owner: String,
    pub repo_name: String,
}

/// What became of a claimed digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestState {
    /// Claimed; sending has not finished (or the process stopped while sending).
    Claimed,
    /// The mail transport accepted it.
    Sent,
    /// The mail transport refused it or timed out. It is not retried.
    Failed,
    /// Nothing was left to send once access and preferences were checked again.
    Empty,
}

impl DigestState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Empty => "empty",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Claimed, Self::Sent, Self::Failed, Self::Empty].into_iter().find(|state| state.as_str() == value)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CandidateRow {
    account_id: i64,
    delivery: Option<String>,
    digest_hour: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct ItemRow {
    repo_id: i64,
    issue_id: i64,
    reason: String,
    headline: String,
    excerpt: String,
    anchor: Option<String>,
    created_at: i64,
    number: i64,
    kind: String,
    title: String,
    owner_name: String,
    repo_name: String,
}

impl TryFrom<ItemRow> for DigestItem {
    type Error = StorageError;

    fn try_from(row: ItemRow) -> Result<Self, Self::Error> {
        let corrupt = |what: &str, detail: &str| StorageError::Corrupt(format!("{what}: {detail}"));
        Ok(Self {
            repo: RepoId(row.repo_id),
            issue: IssueId(row.issue_id),
            reason: NotificationReason::parse(&row.reason).ok_or_else(|| corrupt("digest reason", &row.reason))?,
            headline: row.headline,
            excerpt: row.excerpt,
            anchor: row.anchor,
            created_at: Timestamp(row.created_at),
            number: IssueNumber::try_from(row.number).map_err(|e| corrupt("issue number", &e.to_string()))?,
            kind: IssueKind::parse(&row.kind).ok_or_else(|| corrupt("issue kind", &row.kind))?,
            title: row.title,
            owner: row.owner_name,
            repo_name: row.repo_name,
        })
    }
}

/// Parses a stored delivery; `None` (no settings row) is immediate.
pub(crate) fn parse_delivery(value: Option<&str>) -> Result<EmailDelivery, StorageError> {
    value.map_or(Ok(EmailDelivery::Immediate), |value| {
        EmailDelivery::parse(value).ok_or_else(|| StorageError::Corrupt(format!("email delivery: {value}")))
    })
}

/// Parses a stored digest hour (the column's `CHECK` keeps it within 0–23).
pub(crate) fn parse_hour(value: Option<i64>) -> Result<Option<u8>, StorageError> {
    value
        .map(|hour| {
            u8::try_from(hour)
                .ok()
                .filter(|hour| *hour <= rbit_core::config::MAX_DIGEST_HOUR)
                .ok_or_else(|| StorageError::Corrupt(format!("digest hour: {hour}")))
        })
        .transpose()
}

fn count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[derive(Debug, Clone)]
pub struct EmailDigestStore {
    pool: SqlitePool,
}

impl EmailDigestStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Queues `items` for their recipients' next digests, in one transaction.
    pub async fn queue(&self, items: &[NewDigestItem]) -> Result<(), StorageError> {
        if items.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for item in items {
            sqlx::query(
                "INSERT INTO notification_digest_items \
                 (account_id, repo_id, issue_id, reason, headline, excerpt, anchor, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item.account.0)
            .bind(item.repo.0)
            .bind(item.issue.0)
            .bind(item.reason.as_str())
            .bind(&item.headline)
            .bind(&item.excerpt)
            .bind(item.anchor.as_deref())
            .bind(item.created_at.0)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// How many items wait for `account`'s next digest.
    pub async fn pending(&self, account: AccountId) -> Result<u64, StorageError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_digest_items WHERE account_id = ? AND digest_id IS NULL")
            .bind(account.0)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(n).unwrap_or_default())
    }

    /// Up to `limit` users with pending items whose id is greater than `after`, in id
    /// order, with their digest settings (page through with the last id returned).
    pub async fn candidates(&self, after: AccountId, limit: u32) -> Result<Vec<DigestCandidate>, StorageError> {
        let rows: Vec<CandidateRow> = sqlx::query_as(
            "SELECT p.account_id, s.delivery, s.digest_hour FROM \
             (SELECT DISTINCT account_id FROM notification_digest_items WHERE digest_id IS NULL AND account_id > ? \
              ORDER BY account_id LIMIT ?) p \
             LEFT JOIN notification_email_settings s ON s.account_id = p.account_id ORDER BY p.account_id",
        )
        .bind(after.0)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(DigestCandidate {
                    account: AccountId(row.account_id),
                    delivery: parse_delivery(row.delivery.as_deref())?,
                    digest_hour: parse_hour(row.digest_hour)?,
                })
            })
            .collect()
    }

    /// Claims `account`'s digest for the period starting at `period`: the items queued
    /// before `period` whose thread is still unread in the app are attached to it; items of
    /// threads read since are deleted. When nothing is left, no digest is recorded and
    /// later items wait for the next period.
    pub async fn claim(&self, account: AccountId, period: Timestamp, at: Timestamp) -> Result<Claim, StorageError> {
        let mut tx = self.pool.begin().await?;
        let id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO notification_digests (account_id, period, created_at) VALUES (?, ?, ?) \
             ON CONFLICT (account_id, period) DO NOTHING RETURNING id",
        )
        .bind(account.0)
        .bind(period.0)
        .bind(at.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(id) = id else { return Ok(Claim::Taken) };
        let skipped_read = sqlx::query(
            "DELETE FROM notification_digest_items WHERE account_id = ?1 AND digest_id IS NULL AND created_at < ?2 \
             AND NOT EXISTS (SELECT 1 FROM notifications n WHERE n.user_id = ?1 \
                             AND n.issue_id = notification_digest_items.issue_id AND n.unread = 1)",
        )
        .bind(account.0)
        .bind(period.0)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let items = sqlx::query(
            "UPDATE notification_digest_items SET digest_id = ?1 WHERE account_id = ?2 AND digest_id IS NULL AND created_at < ?3",
        )
        .bind(id)
        .bind(account.0)
        .bind(period.0)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let claimed = if items == 0 {
            sqlx::query("DELETE FROM notification_digests WHERE id = ?").bind(id).execute(&mut *tx).await?;
            Claim::Nothing { skipped_read }
        } else {
            sqlx::query("UPDATE notification_digests SET items = ? WHERE id = ?").bind(count(items)).bind(id).execute(&mut *tx).await?;
            Claim::Claimed { digest: ClaimedDigest { id, account, period, items }, skipped_read }
        };
        tx.commit().await?;
        Ok(claimed)
    }

    /// The items of digest `id`, grouped by repository (owner and name) and thread, oldest
    /// first within a thread.
    pub async fn items(&self, id: i64) -> Result<Vec<DigestItem>, StorageError> {
        let rows: Vec<ItemRow> = sqlx::query_as(
            "SELECT d.repo_id, d.issue_id, d.reason, d.headline, d.excerpt, d.anchor, d.created_at, \
             i.number, i.kind, i.title, o.name AS owner_name, r.name AS repo_name \
             FROM notification_digest_items d JOIN issues i ON i.id = d.issue_id JOIN repositories r ON r.id = d.repo_id \
             JOIN accounts o ON o.id = r.owner_id WHERE d.digest_id = ? \
             ORDER BY o.name COLLATE NOCASE, r.name COLLATE NOCASE, d.repo_id, i.number, d.created_at, d.id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(DigestItem::try_from).collect()
    }

    /// Records what became of digest `id`.
    pub async fn finish(&self, id: i64, state: DigestState, at: Timestamp) -> Result<(), StorageError> {
        sqlx::query("UPDATE notification_digests SET state = ?, finished_at = ? WHERE id = ?")
            .bind(state.as_str())
            .bind(at.0)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The state of `account`'s digest for `period`, if one was claimed.
    pub async fn state(&self, account: AccountId, period: Timestamp) -> Result<Option<DigestState>, StorageError> {
        let state: Option<String> = sqlx::query_scalar("SELECT state FROM notification_digests WHERE account_id = ? AND period = ?")
            .bind(account.0)
            .bind(period.0)
            .fetch_optional(&self.pool)
            .await?;
        state.map(|value| DigestState::parse(&value).ok_or_else(|| StorageError::Corrupt(format!("digest state: {value}")))).transpose()
    }

    /// Deletes digests (with their items) and unclaimed items created before `before`;
    /// returns how many rows.
    pub async fn prune(&self, before: Timestamp) -> Result<u64, StorageError> {
        let mut tx = self.pool.begin().await?;
        let digests =
            sqlx::query("DELETE FROM notification_digests WHERE created_at < ?").bind(before.0).execute(&mut *tx).await?.rows_affected();
        let items = sqlx::query("DELETE FROM notification_digest_items WHERE digest_id IS NULL AND created_at < ?")
            .bind(before.0)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(digests + items)
    }
}
