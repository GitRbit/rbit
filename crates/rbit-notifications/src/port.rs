//! Static borrowed persistence/visibility boundaries; no SQL or broad runtime context.
use crate::{MentionedAccount, Notification, NotificationReason, NotificationRecipient, Subscription, ThreadText};
use rbit_collaboration::{
    comments::CommentId,
    threads::{IssueId, IssueText},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use std::{collections::HashMap, future::Future};

pub trait InboxRepository: Sync {
    type Error: Send;
    fn has_unread(&self, user: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn mark_issue_read(&self, user: AccountId, issue: IssueId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn thread_state(
        &self,
        issue: IssueId,
        user: AccountId,
    ) -> impl Future<Output = Result<(Option<Subscription>, bool), Self::Error>> + Send;
    fn subscription(&self, issue: IssueId, user: AccountId) -> impl Future<Output = Result<Option<Subscription>, Self::Error>> + Send;
    fn set_subscribed(
        &self,
        issue: IssueId,
        user: AccountId,
        subscribed: bool,
        now: Timestamp,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn list(
        &self,
        user: AccountId,
        unread_only: bool,
        limit: u32,
        offset: u64,
    ) -> impl Future<Output = Result<Vec<Notification>, Self::Error>> + Send;
    fn count(&self, user: AccountId, unread_only: bool) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn find(&self, user: AccountId, id: i64) -> impl Future<Output = Result<Option<Notification>, Self::Error>> + Send;
    fn delete(&self, user: AccountId, ids: &[i64]) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn mark_read(&self, user: AccountId, id: i64) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn mark_all_read(&self, user: AccountId, now: Timestamp) -> impl Future<Output = Result<u64, Self::Error>> + Send;
}
/// Bound by composition to the exact accepted viewer; no reauthorization shortcut.
pub trait InboxAccess: Sync {
    type Error: Send;
    type Repository: Send + Sync;
    fn readable_among(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashMap<RepoId, Self::Repository>, Self::Error>> + Send;
    fn find_repository(&self, repo: RepoId) -> impl Future<Output = Result<Option<Self::Repository>, Self::Error>> + Send;
    fn can_read(&self, repo: &Self::Repository) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// The write side of the inbox: participation, subscriber reads, recorded deliveries
/// and retention. Implementations keep the original SQL transactions and batching.
pub trait DeliveryRepository: Sync {
    type Error: Send;
    /// Subscribes `user`, or strengthens an active subscription; an unsubscribe sticks.
    fn participate(
        &self,
        issue: IssueId,
        user: AccountId,
        reason: NotificationReason,
        at: Timestamp,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn subscribers(&self, issue: IssueId) -> impl Future<Output = Result<Vec<(AccountId, NotificationReason)>, Self::Error>> + Send;
    /// Those among `users` with any subscription row for `issue`, subscribed or not.
    fn known_participants(&self, issue: IssueId, users: &[AccountId]) -> impl Future<Output = Result<Vec<AccountId>, Self::Error>> + Send;
    /// Records one activity for already-authorized recipients; returns each one's reason.
    fn notify_many(
        &self,
        issue: IssueId,
        repo: RepoId,
        actor: Option<AccountId>,
        at: Timestamp,
        recipients: &[NotificationRecipient],
    ) -> impl Future<Output = Result<Vec<(AccountId, NotificationReason)>, Self::Error>> + Send;
    /// Deletes read rows last updated before `before`.
    fn prune_read(&self, before: Timestamp) -> impl Future<Output = Result<u64, Self::Error>> + Send;
}

/// Read-only thread content the fan-out needs. Mention scanning stays with the shared
/// Markdown parser; this port only resolves its result to accounts.
pub trait ActivitySource: Sync {
    type Error: Send;
    fn thread(&self, issue: IssueId) -> impl Future<Output = Result<Option<ThreadText>, Self::Error>> + Send;
    fn assignees(&self, issue: IssueId) -> impl Future<Output = Result<Vec<AccountId>, Self::Error>> + Send;
    fn comment_body(&self, issue: IssueId, comment: CommentId) -> impl Future<Output = Result<Option<IssueText>, Self::Error>> + Send;
    /// Existing accounts mentioned in Markdown `text`; empty without a query when the
    /// text mentions nobody.
    fn mentioned(&self, text: &str) -> impl Future<Output = Result<Vec<MentionedAccount>, Self::Error>> + Send;
}

/// Current authorization of recipients, refreshed by the caller for every batch.
pub trait RecipientAccess: Sync {
    type Error: Send;
    /// Of `users`, the active ones who may read `repo` now; `None` once `repo` is gone.
    fn readers(&self, repo: RepoId, users: &[AccountId]) -> impl Future<Output = Result<Option<Vec<AccountId>>, Self::Error>> + Send;
}
