//! Pull merge presentation, request and failure contracts; no I/O or policy execution.
use crate::ServiceError;
use rbit_core::{
    issue::MergeStrategy,
    protection::MergeRules,
    pull::{MergeEvaluation, Mergeability, ReviewTally},
    status::CommitStatus,
};

/// The merge box as one viewer sees it.
#[derive(Debug, Clone)]
pub struct MergeBox {
    pub evaluation: MergeEvaluation,
    pub rules: MergeRules,
    pub tally: ReviewTally,
    /// The mergeability shown: the stored one, or `Unknown` when it ran on another base.
    pub mergeable: Mergeability,
    pub conflict_paths: Vec<String>,
    /// The latest status of each context on the head commit, by context (milestone 3, S).
    pub statuses: Vec<CommitStatus>,
    /// The contexts required of this head: the rules' ones, without those no workflow will
    /// report for it (agent plan steps 2.2 and 4.2).
    pub required: Vec<String>,
    pub head_branch_exists: bool,
    /// The viewer may press the button (write access to the base repository).
    pub can_merge: bool,
    /// The viewer may delete the head branch after merging.
    pub can_delete_head: bool,
    pub default_title: String,
    pub default_message: String,
    pub default_squash_title: String,
    pub default_squash_message: String,
}

/// A merge as requested.
#[derive(Debug, Clone)]
pub struct MergeInput {
    pub strategy: MergeStrategy,
    /// Empty: the default for the strategy.
    pub title: String,
    pub message: String,
    /// The head the merger saw; a different current head refuses the merge.
    pub expected_head: Option<String>,
    /// The base branch the merger saw; a different current base refuses the merge.
    pub expected_base: Option<String>,
    pub delete_head_branch: bool,
    /// Merge despite unmet protection rules (only for mergers allowed to bypass).
    pub bypass: bool,
}

/// Why a merge did not happen.
#[derive(Debug, thiserror::Error)]
pub enum MergeFailure {
    /// The head is not the one the merger reviewed (`409`).
    #[error("the head branch was modified; review the new changes and try again")]
    HeadChanged,
    /// The base branch is not the one the merger saw (`409`).
    #[error("the base branch was changed; review the pull request and try again")]
    BaseChanged,
    /// A requirement is unmet (`405`); the message says which.
    #[error("{0}")]
    Blocked(String),
    #[error("{0}")]
    Service(#[from] ServiceError),
}

impl From<rbit_storage::StorageError> for MergeFailure {
    fn from(error: rbit_storage::StorageError) -> Self {
        Self::Service(error.into())
    }
}
