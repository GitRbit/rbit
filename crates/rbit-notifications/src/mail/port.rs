//! Static borrowed boundaries of email delivery. Composition renders, throttles and
//! transports; stores keep SQL claims; access owners decide readability.
use super::{
    activity::MailActivity,
    digest::DigestSection,
    records::{Claim, ClaimedDigest, DigestItem, DigestState, MailRecipient, NewDigestItem, PendingDigest},
};
use crate::NotificationReason;
use rbit_collaboration::threads::IssueId;
use rbit_contracts::{AccountId, RepoId, Timestamp};
use std::{collections::HashSet, future::Future};

/// Everything one activity's emails need, bound by composition to that thread.
pub trait MailOutbox: Sync {
    type Error: Send;
    type Repository: Send + Sync;
    type User: Send + Sync;
    type Mail: Send;
    /// The repository as it is now; `None` once it is gone.
    fn repository(&self, repo: RepoId) -> impl Future<Output = Result<Option<Self::Repository>, Self::Error>> + Send;
    /// Fresh recipient rows: active, verified, with their settings.
    fn recipients(&self, accounts: &[AccountId]) -> impl Future<Output = Result<Vec<MailRecipient<Self::User>>, Self::Error>> + Send;
    /// Those among `users` who may read `repo` now, with current grants.
    fn readers(
        &self,
        repo: &Self::Repository,
        users: &[&MailRecipient<Self::User>],
    ) -> impl Future<Output = Result<HashSet<AccountId>, Self::Error>> + Send;
    fn render(
        &self,
        repo: &Self::Repository,
        user: &MailRecipient<Self::User>,
        activity: &MailActivity,
        reason: NotificationReason,
    ) -> impl Future<Output = Result<Self::Mail, Self::Error>> + Send;
    /// Claims each account's dedup key once; returns the accounts claimed now.
    fn claim(
        &self,
        accounts: &[AccountId],
        issue: IssueId,
        key: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<HashSet<AccountId>, Self::Error>> + Send;
    fn queue_digest(&self, items: &[NewDigestItem]) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Hands one claimed email to the fire-and-forget queue, within the user's budget.
    fn send(&self, user: &MailRecipient<Self::User>, mail: Self::Mail);
}

/// Everything a digest run needs. Delivery is direct, so the transport's verdict counts.
pub trait DigestOutbox: Sync {
    type Error: Send;
    type User: Send + Sync;
    type Mail: Send;
    fn candidates(&self, after: AccountId, limit: u32) -> impl Future<Output = Result<Vec<PendingDigest>, Self::Error>> + Send;
    fn claim(&self, account: AccountId, period: Timestamp, now: Timestamp) -> impl Future<Output = Result<Claim, Self::Error>> + Send;
    fn finish(&self, digest: i64, state: DigestState, now: Timestamp) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn recipient(&self, account: AccountId) -> impl Future<Output = Result<Option<MailRecipient<Self::User>>, Self::Error>> + Send;
    fn items(&self, digest: i64) -> impl Future<Output = Result<Vec<DigestItem>, Self::Error>> + Send;
    /// Whether `user` may read `repo` now (`false` once it is gone).
    fn can_read(&self, user: &MailRecipient<Self::User>, repo: RepoId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn render(
        &self,
        user: &MailRecipient<Self::User>,
        digest: &ClaimedDigest,
        weekly: bool,
        sections: Vec<DigestSection>,
    ) -> impl Future<Output = Result<Self::Mail, Self::Error>> + Send;
    /// Whether the transport accepted it; `notifications` is for the log.
    fn deliver(&self, user: &MailRecipient<Self::User>, mail: Self::Mail, notifications: usize) -> impl Future<Output = bool> + Send;
    /// A digest that could not be built is failed, not retried; composition logs why.
    fn build_failed(&self, account: AccountId, error: &Self::Error);
    /// Counts a digest the transport accepted (`true`) or refused or could not take.
    fn record(&self, sent: bool);
}
