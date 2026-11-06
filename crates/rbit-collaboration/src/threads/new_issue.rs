//! A validated thread row to insert; persistence assigns its number.

use super::{IssueKind, IssueText, IssueTitle};
use rbit_contracts::{AccountId, RepoId, Timestamp};

/// An issue or pull request to insert. Its number is assigned on insertion.
#[derive(Debug, Clone)]
pub struct NewIssue {
    pub repo: RepoId,
    pub kind: IssueKind,
    pub title: IssueTitle,
    pub body: IssueText,
    pub author: AccountId,
    pub at: Timestamp,
}
