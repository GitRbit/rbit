//! Shared human and Actions comment records, retaining their legacy wire identity.

use crate::threads::{IssueId, IssueText};
use rbit_contracts::{AccountId, Timestamp};
use serde::{Deserialize, Serialize};

/// Database id of a comment on an issue or pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommentId(pub i64);

/// A comment on an issue or pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: CommentId,
    pub issue_id: IssueId,
    /// `None` once the author's account is deleted, and for rbit Actions.
    pub author: Option<AccountId>,
    pub body: IssueText,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// `None` until the comment is edited.
    pub edited_at: Option<Timestamp>,
    // Actions: task tokens.
    pub source: CommentSource,
}

// Actions: task tokens.
/// Who wrote a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentSource {
    /// A person (or anything acting with their credentials).
    #[default]
    User,
    /// A workflow task, with its token. Such a comment has no author.
    Actions,
}

impl CommentSource {
    /// The stored value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Actions => "actions",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::User, Self::Actions].into_iter().find(|source| source.as_str() == value)
    }
}
