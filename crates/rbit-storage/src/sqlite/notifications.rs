//! In-app notifications: thread subscriptions and each user's inbox (migration
//! `0019_notifications.sql`).
//!
//! Milestone 2 workstream I (issues) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! A subscription row says that a user takes part in a thread and why
//! ([`NotificationReason`]); `subscribed = false` is an explicit unsubscribe, which
//! automatic participation never overrides. The inbox has one row per (user, thread),
//! upserted unread on new activity with the strongest reason. Who may receive what is
//! the service's decision; this store only records it.

use std::collections::{BTreeMap, HashMap};

use rbit_core::{AccountId, IssueId, IssueKind, IssueNumber, IssueState, RepoId, StateReason, Timestamp};
use sqlx::SqlitePool;

use crate::StorageError;

/// Maximum recipients in one fan-out transaction, keeping SQLite's writer available
/// between batches even for threads with many subscribers.
pub const NOTIFICATION_BATCH_SIZE: usize = 100;

/// Why a user hears about a thread, weakest first; the strongest wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NotificationReason {
    /// Subscribed by hand.
    Manual,
    Comment,
    /// Reviewed the pull request.
    Reviewed,
    /// Opened the thread.
    Author,
    Mention,
    Assigned,
    ReviewRequested,
}

impl NotificationReason {
    pub const ALL: [Self; 7] =
        [Self::Manual, Self::Comment, Self::Reviewed, Self::Author, Self::Mention, Self::Assigned, Self::ReviewRequested];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Comment => "comment",
            Self::Reviewed => "reviewed",
            Self::Author => "author",
            Self::Mention => "mention",
            Self::Assigned => "assigned",
            Self::ReviewRequested => "review_requested",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.as_str() == value)
    }

    /// How the inbox explains it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Manual => "Subscribed",
            Self::Comment => "Commented",
            Self::Reviewed => "Reviewed",
            Self::Author => "Author",
            Self::Mention => "Mentioned",
            Self::Assigned => "Assigned",
            Self::ReviewRequested => "Review requested",
        }
    }
}

/// The rank of reason column `$column` in SQL (matching [`NotificationReason`]'s order).
macro_rules! rank {
    ($column:literal) => {
        concat!(
            "(CASE ",
            $column,
            " WHEN 'manual' THEN 0 WHEN 'comment' THEN 1 WHEN 'reviewed' THEN 2 WHEN 'author' THEN 3 \
             WHEN 'mention' THEN 4 WHEN 'assigned' THEN 5 WHEN 'review_requested' THEN 6 ELSE 0 END)"
        )
    };
}

/// A user's subscription to a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription {
    pub reason: NotificationReason,
    pub subscribed: bool,
}

/// One already-authorized recipient of an activity. Direct participation is optional;
/// an explicit unsubscribe is never overwritten by automatic participation.
#[derive(Debug, Clone, Copy)]
pub struct NotificationRecipient {
    pub user: AccountId,
    pub reason: NotificationReason,
    pub participation: Option<NotificationReason>,
}

/// One inbox row with what it needs to be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: i64,
    pub user: AccountId,
    pub issue: IssueId,
    pub repo: RepoId,
    pub reason: NotificationReason,
    pub unread: bool,
    pub last_actor: Option<AccountId>,
    pub updated_at: Timestamp,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub state: IssueState,
    pub state_reason: Option<StateReason>,
    pub title: String,
    /// The repository's owner and name.
    pub owner: String,
    pub repo_name: String,
}

impl Notification {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo_name)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct NotificationRow {
    id: i64,
    user_id: i64,
    issue_id: i64,
    repo_id: i64,
    reason: String,
    unread: bool,
    last_actor_id: Option<i64>,
    updated_at: i64,
    number: i64,
    kind: String,
    state: String,
    state_reason: Option<String>,
    title: String,
    owner_name: String,
    repo_name: String,
}

impl TryFrom<NotificationRow> for Notification {
    type Error = StorageError;

    fn try_from(row: NotificationRow) -> Result<Self, Self::Error> {
        let corrupt = |what: &str, detail: &str| StorageError::Corrupt(format!("{what}: {detail}"));
        Ok(Self {
            id: row.id,
            user: AccountId(row.user_id),
            issue: IssueId(row.issue_id),
            repo: RepoId(row.repo_id),
            reason: NotificationReason::parse(&row.reason).ok_or_else(|| corrupt("notification reason", &row.reason))?,
            unread: row.unread,
            last_actor: row.last_actor_id.map(AccountId),
            updated_at: Timestamp(row.updated_at),
            number: IssueNumber::try_from(row.number).map_err(|e| corrupt("issue number", &e.to_string()))?,
            kind: IssueKind::parse(&row.kind).ok_or_else(|| corrupt("issue kind", &row.kind))?,
            state: IssueState::parse(&row.state).ok_or_else(|| corrupt("issue state", &row.state))?,
            state_reason: row
                .state_reason
                .map(|reason| StateReason::parse(&reason).ok_or_else(|| corrupt("state reason", &reason)))
                .transpose()?,
            title: row.title,
            owner: row.owner_name,
            repo_name: row.repo_name,
        })
    }
}

macro_rules! select_notifications {
    ($where:literal) => {
        concat!(
            "SELECT n.id, n.user_id, n.issue_id, n.repo_id, n.reason, n.unread, n.last_actor_id, n.updated_at, \
             i.number, i.kind, i.state, i.state_reason, i.title, o.name AS owner_name, r.name AS repo_name \
             FROM notifications n JOIN issues i ON i.id = n.issue_id JOIN repositories r ON r.id = n.repo_id \
             JOIN accounts o ON o.id = r.owner_id ",
            $where
        )
    };
}

fn json_ids(values: impl IntoIterator<Item = i64>) -> String {
    let values: Vec<i64> = values.into_iter().collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned())
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct NotificationStore {
    pool: SqlitePool,
}

impl NotificationStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ------------------------------------------------------------------ subscriptions

    /// Records that `user` takes part in `issue` for `reason`: subscribes them if they
    /// had no row, or strengthens the reason of an active subscription. An explicit
    /// unsubscribe is left alone.
    pub async fn participate(
        &self,
        issue: IssueId,
        user: AccountId,
        reason: NotificationReason,
        at: Timestamp,
    ) -> Result<(), StorageError> {
        sqlx::query(concat!(
            "INSERT INTO issue_subscriptions (issue_id, user_id, reason, subscribed, updated_at) VALUES (?1, ?2, ?3, 1, ?4) \
             ON CONFLICT (issue_id, user_id) DO UPDATE SET reason = excluded.reason, updated_at = excluded.updated_at \
             WHERE subscribed = 1 AND ",
            rank!("excluded.reason"),
            " > ",
            rank!("issue_subscriptions.reason")
        ))
        .bind(issue.0)
        .bind(user.0)
        .bind(reason.as_str())
        .bind(at.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Subscribes or unsubscribes `user` by hand. Subscribing keeps a stronger existing
    /// reason; unsubscribing sticks until they subscribe again.
    pub async fn set_subscribed(&self, issue: IssueId, user: AccountId, subscribed: bool, at: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO issue_subscriptions (issue_id, user_id, reason, subscribed, updated_at) VALUES (?1, ?2, 'manual', ?3, ?4) \
             ON CONFLICT (issue_id, user_id) DO UPDATE SET subscribed = excluded.subscribed, updated_at = excluded.updated_at",
        )
        .bind(issue.0)
        .bind(user.0)
        .bind(subscribed)
        .bind(at.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn subscription(&self, issue: IssueId, user: AccountId) -> Result<Option<Subscription>, StorageError> {
        let row: Option<(String, bool)> =
            sqlx::query_as("SELECT reason, subscribed FROM issue_subscriptions WHERE issue_id = ? AND user_id = ?")
                .bind(issue.0)
                .bind(user.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(reason, subscribed)| {
            let reason =
                NotificationReason::parse(&reason).ok_or_else(|| StorageError::Corrupt(format!("subscription reason: {reason}")))?;
            Ok(Subscription { reason, subscribed })
        })
        .transpose()
    }

    /// Everyone subscribed to `issue`, with their reason.
    pub async fn subscribers(&self, issue: IssueId) -> Result<Vec<(AccountId, NotificationReason)>, StorageError> {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT user_id, reason FROM issue_subscriptions WHERE issue_id = ? AND subscribed = 1 ORDER BY user_id")
                .bind(issue.0)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().filter_map(|(user, reason)| Some((AccountId(user), NotificationReason::parse(&reason)?))).collect())
    }

    /// Those among `users` with a subscription row for `issue` (subscribed or not).
    pub async fn known_participants(&self, issue: IssueId, users: &[AccountId]) -> Result<Vec<AccountId>, StorageError> {
        if users.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM issue_subscriptions WHERE issue_id = ? AND user_id IN (SELECT value FROM json_each(?)) ORDER BY user_id",
        )
        .bind(issue.0)
        .bind(json_ids(users.iter().map(|u| u.0)))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(AccountId).collect())
    }

    // ------------------------------------------------------------------ the inbox

    /// Records an activity for already-authorized recipients in bounded transactions.
    /// Returns each recipient's event/subscription reason, independently of a stronger
    /// reason already in their unread inbox. Duplicate recipients are combined.
    pub async fn notify_many(
        &self,
        issue: IssueId,
        repo: RepoId,
        actor: Option<AccountId>,
        at: Timestamp,
        recipients: &[NotificationRecipient],
    ) -> Result<Vec<(AccountId, NotificationReason)>, StorageError> {
        let mut unique = BTreeMap::<AccountId, NotificationRecipient>::new();
        for recipient in recipients {
            unique
                .entry(recipient.user)
                .and_modify(|existing| {
                    existing.reason = existing.reason.max(recipient.reason);
                    existing.participation = existing.participation.max(recipient.participation);
                })
                .or_insert(*recipient);
        }
        let recipients: Vec<_> = unique.into_values().collect();
        let mut notified = Vec::with_capacity(recipients.len());
        for batch in recipients.chunks(NOTIFICATION_BATCH_SIZE) {
            notified.extend(self.notify_batch(issue, repo, actor, at, batch).await?);
        }
        Ok(notified)
    }

    async fn notify_batch(
        &self,
        issue: IssueId,
        repo: RepoId,
        actor: Option<AccountId>,
        at: Timestamp,
        recipients: &[NotificationRecipient],
    ) -> Result<Vec<(AccountId, NotificationReason)>, StorageError> {
        let input: Vec<_> =
            recipients.iter().map(|r| (r.user.0, r.reason.as_str(), r.participation.map(NotificationReason::as_str))).collect();
        let input = serde_json::to_string(&input).map_err(|error| StorageError::Corrupt(format!("notification recipients: {error}")))?;
        // Acquire the writer before reading subscriptions: another subscription update
        // must not turn a read-then-write snapshot into SQLITE_BUSY_SNAPSHOT.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if recipients.iter().any(|recipient| recipient.participation.is_some()) {
            sqlx::query(concat!(
                "INSERT INTO issue_subscriptions (issue_id, user_id, reason, subscribed, updated_at) \
                 SELECT ?1, json_extract(value, '$[0]'), json_extract(value, '$[2]'), 1, ?2 FROM json_each(?3) \
                 WHERE json_extract(value, '$[2]') IS NOT NULL \
                 ON CONFLICT (issue_id, user_id) DO UPDATE SET reason = excluded.reason, updated_at = excluded.updated_at \
                 WHERE subscribed = 1 AND ",
                rank!("excluded.reason"),
                " > ",
                rank!("issue_subscriptions.reason")
            ))
            .bind(issue.0)
            .bind(at.0)
            .bind(&input)
            .execute(&mut *tx)
            .await?;
        }
        let subscriptions: Vec<(i64, String)> = sqlx::query_as(
            "SELECT user_id, reason FROM issue_subscriptions WHERE issue_id = ?1 AND subscribed = 1 \
             AND user_id IN (SELECT json_extract(value, '$[0]') FROM json_each(?2))",
        )
        .bind(issue.0)
        .bind(&input)
        .fetch_all(&mut *tx)
        .await?;
        let subscriptions: HashMap<AccountId, NotificationReason> = subscriptions
            .into_iter()
            .map(|(user, reason)| {
                let reason =
                    NotificationReason::parse(&reason).ok_or_else(|| StorageError::Corrupt(format!("subscription reason: {reason}")))?;
                Ok((AccountId(user), reason))
            })
            .collect::<Result<_, StorageError>>()?;
        let notified: Vec<_> = recipients
            .iter()
            .map(|recipient| {
                let reason = subscriptions.get(&recipient.user).map_or(recipient.reason, |reason| (*reason).max(recipient.reason));
                (recipient.user, reason)
            })
            .collect();
        let rows: Vec<_> = notified.iter().map(|(user, reason)| (user.0, reason.as_str())).collect();
        let rows = serde_json::to_string(&rows).map_err(|error| StorageError::Corrupt(format!("notification recipients: {error}")))?;
        sqlx::query(concat!(
            "INSERT INTO notifications (user_id, issue_id, repo_id, reason, unread, last_actor_id, updated_at) \
             SELECT json_extract(value, '$[0]'), ?1, ?2, json_extract(value, '$[1]'), 1, ?3, ?4 FROM json_each(?5) WHERE 1 \
             ON CONFLICT (user_id, issue_id) DO UPDATE SET \
             reason = CASE WHEN notifications.unread = 1 AND ",
            rank!("notifications.reason"),
            " > ",
            rank!("excluded.reason"),
            " THEN notifications.reason ELSE excluded.reason END, \
             unread = 1, last_actor_id = excluded.last_actor_id, updated_at = MAX(notifications.updated_at, excluded.updated_at), \
             repo_id = excluded.repo_id"
        ))
        .bind(issue.0)
        .bind(repo.0)
        .bind(actor.map(|a| a.0))
        .bind(at.0)
        .bind(rows)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(notified)
    }

    /// Marks `issue` unread in `user`'s inbox for `reason` (by `actor`). An unread row
    /// keeps the stronger of its reason and `reason`; a read one takes `reason`.
    pub async fn notify(
        &self,
        user: AccountId,
        issue: IssueId,
        repo: RepoId,
        reason: NotificationReason,
        actor: Option<AccountId>,
        at: Timestamp,
    ) -> Result<(), StorageError> {
        sqlx::query(concat!(
            "INSERT INTO notifications (user_id, issue_id, repo_id, reason, unread, last_actor_id, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6) \
             ON CONFLICT (user_id, issue_id) DO UPDATE SET \
             reason = CASE WHEN notifications.unread = 1 AND ",
            rank!("notifications.reason"),
            " > ",
            rank!("excluded.reason"),
            " THEN notifications.reason ELSE excluded.reason END, \
             unread = 1, last_actor_id = excluded.last_actor_id, updated_at = MAX(notifications.updated_at, excluded.updated_at), \
             repo_id = excluded.repo_id"
        ))
        .bind(user.0)
        .bind(issue.0)
        .bind(repo.0)
        .bind(reason.as_str())
        .bind(actor.map(|a| a.0))
        .bind(at.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Whether `user` has anything unread (one indexed `EXISTS`).
    pub async fn has_unread(&self, user: AccountId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM notifications WHERE user_id = ? AND unread = 1)")
            .bind(user.0)
            .fetch_one(&self.pool)
            .await?)
    }

    /// One page of `user`'s inbox (only unread rows with `unread_only`), newest first.
    pub async fn list(&self, user: AccountId, unread_only: bool, limit: u32, offset: u64) -> Result<Vec<Notification>, StorageError> {
        let rows: Vec<NotificationRow> = sqlx::query_as(select_notifications!(
            "WHERE n.user_id = ?1 AND (?2 = 0 OR n.unread = 1) ORDER BY n.updated_at DESC, n.id DESC LIMIT ?3 OFFSET ?4"
        ))
        .bind(user.0)
        .bind(unread_only)
        .bind(i64::from(limit))
        .bind(i64::try_from(offset).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Notification::try_from).collect()
    }

    /// How many rows [`Self::list`] can page through.
    pub async fn count(&self, user: AccountId, unread_only: bool) -> Result<u64, StorageError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND (?2 = 0 OR unread = 1)")
            .bind(user.0)
            .bind(unread_only)
            .fetch_one(&self.pool)
            .await?;
        Ok(count(n))
    }

    /// Notification `id`, only if it is `user`'s.
    pub async fn find(&self, user: AccountId, id: i64) -> Result<Option<Notification>, StorageError> {
        let row: Option<NotificationRow> = sqlx::query_as(select_notifications!("WHERE n.id = ?1 AND n.user_id = ?2"))
            .bind(id)
            .bind(user.0)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Notification::try_from).transpose()
    }

    /// Marks `user`'s row about `issue` read; `false` when there was nothing unread.
    pub async fn mark_issue_read(&self, user: AccountId, issue: IssueId) -> Result<bool, StorageError> {
        let changed = sqlx::query("UPDATE notifications SET unread = 0 WHERE user_id = ? AND issue_id = ? AND unread = 1")
            .bind(user.0)
            .bind(issue.0)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(changed > 0)
    }

    /// Marks notification `id` of `user` read; `false` when it is not theirs.
    pub async fn mark_read(&self, user: AccountId, id: i64) -> Result<bool, StorageError> {
        let changed = sqlx::query("UPDATE notifications SET unread = 0 WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user.0)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(changed > 0)
    }

    /// Marks every row of `user` updated at or before `before` read; returns how many changed.
    pub async fn mark_all_read(&self, user: AccountId, before: Timestamp) -> Result<u64, StorageError> {
        Ok(sqlx::query("UPDATE notifications SET unread = 0 WHERE user_id = ? AND unread = 1 AND updated_at <= ?")
            .bind(user.0)
            .bind(before.0)
            .execute(&self.pool)
            .await?
            .rows_affected())
    }

    /// Deletes `user`'s rows among `ids` (rows of repositories they can no longer read).
    pub async fn delete(&self, user: AccountId, ids: &[i64]) -> Result<u64, StorageError> {
        if ids.is_empty() {
            return Ok(0);
        }
        Ok(sqlx::query("DELETE FROM notifications WHERE user_id = ? AND id IN (SELECT value FROM json_each(?))")
            .bind(user.0)
            .bind(json_ids(ids.iter().copied()))
            .execute(&self.pool)
            .await?
            .rows_affected())
    }

    /// Deletes read rows last updated before `before`; returns how many.
    pub async fn prune_read(&self, before: Timestamp) -> Result<u64, StorageError> {
        Ok(sqlx::query("DELETE FROM notifications WHERE unread = 0 AND updated_at < ?")
            .bind(before.0)
            .execute(&self.pool)
            .await?
            .rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_round_trip_in_strength_order() {
        for reason in NotificationReason::ALL {
            assert_eq!(NotificationReason::parse(reason.as_str()), Some(reason));
        }
        assert!(NotificationReason::ReviewRequested > NotificationReason::Assigned);
        assert!(NotificationReason::Assigned > NotificationReason::Mention);
        assert!(NotificationReason::Mention > NotificationReason::Author);
        assert!(NotificationReason::Author > NotificationReason::Comment);
        assert!(NotificationReason::Comment > NotificationReason::Manual);
        assert_eq!(NotificationReason::parse("nope"), None);
    }
}
