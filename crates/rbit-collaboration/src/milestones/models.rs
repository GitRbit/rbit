//! Milestone catalog records; thread attachment records remain with their existing adapter.

use crate::IssueState;
use rbit_contracts::{RepoId, Timestamp};
use serde::{Deserialize, Serialize};

/// Database id of a milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MilestoneId(pub i64);

/// A repository's milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    pub id: MilestoneId,
    pub repo_id: RepoId,
    pub title: String,
    pub description: String,
    /// `YYYY-MM-DD`.
    pub due_on: Option<String>,
    pub state: IssueState,
    pub closed_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A milestone's fields, already validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneFields {
    pub title: String,
    pub description: String,
    pub due_on: Option<String>,
}

/// A milestone with how many of its threads are open and closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneProgress {
    pub milestone: Milestone,
    pub open_issues: u64,
    pub closed_issues: u64,
}

/// A milestone as submitted (`due_on`: `YYYY-MM-DD` or empty).
#[derive(Debug, Clone, Default)]
pub struct MilestoneInput {
    pub title: String,
    pub description: String,
    pub due_on: String,
}

/// The milestone an issue belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneRef {
    pub id: MilestoneId,
    pub title: String,
}
