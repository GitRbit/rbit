//! Static borrowed persistence/visibility boundaries; no SQL or broad runtime context.
use crate::{Notification, Subscription};
use rbit_collaboration::threads::IssueId;
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
