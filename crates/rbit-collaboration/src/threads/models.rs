//! Numbered thread records retain identity models without credential features.

use super::{IssueKind, IssueNumber, IssueText, IssueTitle, StateReason};
use crate::IssueState;
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity::User;
use serde::{Deserialize, Serialize};

/// Database id of an issue or pull request (the `issues` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IssueId(pub i64);

/// An issue or pull request (the shared `issues` row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub id: IssueId,
    pub repo_id: RepoId,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub title: IssueTitle,
    pub body: IssueText,
    /// `None` once the author's account is deleted (shown as a ghost).
    pub author: Option<AccountId>,
    pub state: IssueState,
    /// Set exactly when closed.
    pub state_reason: Option<StateReason>,
    pub closed_at: Option<Timestamp>,
    pub closed_by: Option<AccountId>,
    pub locked: bool,
    pub comment_count: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Issue {
    pub fn is_open(&self) -> bool {
        self.state == IssueState::Open
    }

    pub fn is_pull(&self) -> bool {
        self.kind == IssueKind::Pull
    }

    /// Whether `user` opened it.
    pub fn is_author(&self, user: &User) -> bool {
        self.author == Some(user.id())
    }
}
