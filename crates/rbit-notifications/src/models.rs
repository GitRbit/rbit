//! Feature-owned inbox/subscription records; SQL row decoding remains in storage.
use crate::NotificationReason;
use rbit_collaboration::{
    IssueState,
    threads::{IssueId, IssueKind, IssueNumber, StateReason},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
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
