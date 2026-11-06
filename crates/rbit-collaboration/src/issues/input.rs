//! Issue creation input; metadata is ignored for non-triaging readers.

use crate::{labels::LabelId, milestones::MilestoneId};
use rbit_contracts::AccountId;

/// A new issue as submitted. Labels, assignees and the milestone are applied only when
/// the author may triage (they are ignored otherwise, as on GitHub).
#[derive(Debug, Clone, Default)]
pub struct NewIssueInput {
    pub title: String,
    pub body: String,
    pub labels: Vec<LabelId>,
    pub assignees: Vec<AccountId>,
    pub milestone: Option<MilestoneId>,
}
