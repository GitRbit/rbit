//! Pull request, review and review comment records and the values stored with them.

use std::fmt;

use rbit_contracts::{AccountId, RepoId, Timestamp};
use serde::{Deserialize, Serialize};

use super::ReviewSide;
use crate::threads::IssueId;

/// Conflicting paths stored (and shown) at most per pull request.
pub const MAX_CONFLICT_PATHS: usize = 100;

/// Database id of a pull request review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewId(pub i64);

/// Database id of a review comment on a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewCommentId(pub i64);

/// Generates `as_str`, `parse` and `Display` for a field-less enum stored as text.
macro_rules! stored_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

stored_enum!(
    /// The result of the last trial merge of head into base.
    Mergeability {
        /// Not computed yet, or the head or base moved since.
        Unknown => "unknown",
        Clean => "clean",
        Conflict => "conflict",
        /// The trial merge failed for another reason (logged); retried after the next push.
        Error => "error",
    }
);

stored_enum!(
    /// The combined state of the head commit's statuses (milestone 3, S; see
    /// [`crate::status::checks_status`]): `NotConfigured` when nothing reported.
    ChecksStatus {
        NotConfigured => "not_configured",
        Pending => "pending",
        Passing => "passing",
        Failing => "failing",
    }
);

stored_enum!(
    /// How a pull request was merged.
    MergeStrategy {
        /// A merge commit with two parents.
        Merge => "merge",
        /// One new commit with the head's tree.
        Squash => "squash",
        /// The head's commits replayed onto the base.
        Rebase => "rebase",
    }
);

stored_enum!(
    /// A pull request review's state. `Pending` reviews are visible only to their author.
    ReviewState {
        Pending => "pending",
        Commented => "commented",
        Approved => "approved",
        ChangesRequested => "changes_requested",
        Dismissed => "dismissed",
    }
);

/// A pull request's own row, keyed by its issue row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub issue_id: IssueId,
    /// The base repository (the issue row's repository).
    pub repo_id: RepoId,
    /// `None` once the head repository is deleted (`refs/pull/{n}/head` keeps the commits).
    pub head_repo_id: Option<RepoId>,
    pub head_branch: String,
    pub base_branch: String,
    /// The head commit as of the last push rbit saw (what `refs/pull/{n}/head` points at).
    pub head_sha: String,
    pub is_draft: bool,
    pub mergeable: Mergeability,
    /// The base and head ids the stored mergeability was computed on.
    pub mergeable_base_sha: Option<String>,
    pub mergeable_head_sha: Option<String>,
    /// For [`Mergeability::Conflict`]: at most [`MAX_CONFLICT_PATHS`] paths.
    pub conflict_paths: Vec<String>,
    pub merged_at: Option<Timestamp>,
    pub merged_by: Option<AccountId>,
    /// `None` for a pull request merged outside rbit.
    pub merge_strategy: Option<MergeStrategy>,
    pub merge_commit_sha: Option<String>,
    pub merged_base_sha: Option<String>,
    pub merged_head_sha: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl PullRequest {
    pub fn is_merged(&self) -> bool {
        self.merged_at.is_some()
    }

    /// The head lives in another repository than the base.
    pub fn is_cross_repository(&self) -> bool {
        self.head_repo_id != Some(self.repo_id)
    }

    /// GitHub's `mergeable_state` for an open pull request: `draft`, `dirty` (conflicts),
    /// `blocked` (requirements unmet), `clean` or `unknown`.
    pub fn mergeable_state(&self, blocked: bool) -> &'static str {
        match (self.is_draft, self.mergeable) {
            (true, _) => "draft",
            (false, Mergeability::Conflict) => "dirty",
            (false, Mergeability::Clean) if blocked => "blocked",
            (false, Mergeability::Clean) => "clean",
            (false, Mergeability::Unknown | Mergeability::Error) => "unknown",
        }
    }
}

/// A review of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub id: ReviewId,
    pub pull_id: IssueId,
    /// `None` once the reviewer's account is deleted.
    pub reviewer: Option<AccountId>,
    pub state: ReviewState,
    pub body: String,
    /// The head commit the review was made on.
    pub commit_sha: String,
    pub created_at: Timestamp,
    /// `None` while pending.
    pub submitted_at: Option<Timestamp>,
    pub dismissed_by: Option<AccountId>,
    pub dismissed_at: Option<Timestamp>,
    pub dismissal_message: Option<String>,
}

/// A comment on one diff line, part of a review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComment {
    pub id: ReviewCommentId,
    pub review_id: ReviewId,
    pub pull_id: IssueId,
    pub author: Option<AccountId>,
    pub path: String,
    pub side: ReviewSide,
    /// 1-based line number on `side`.
    pub line: u32,
    /// The head commit the comment was made on.
    pub commit_sha: String,
    /// Up to four lines of the diff ending at the commented line, for display.
    pub diff_hunk: String,
    pub in_reply_to: Option<ReviewCommentId>,
    pub body: String,
    /// The file changed since the comment's commit; shown only in the conversation.
    pub outdated: bool,
    /// Resolution is stored on the root comment; replies inherit its state.
    pub resolved_at: Option<Timestamp>,
    pub resolved_by: Option<AccountId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
