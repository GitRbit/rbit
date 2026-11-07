//! Metadata vectors are moved directly from storage, without another row buffer.
use crate::{labels::LabelSnapshot, milestones::IssueMilestone, threads::IssueId};
use rbit_identity_models::Account;
/// Metadata of one thread, read by one joined statement.
#[derive(Debug, Clone, Default)]
pub struct OneIssueMetadata {
    /// Ordered by label name.
    pub labels: Vec<LabelSnapshot>,
    /// Ordered by account name.
    pub assignees: Vec<Account>,
    pub milestone: Option<IssueMilestone>,
}

/// Three concurrent selected-page queries, in labels/assignees/milestones order.
pub type MetadataRows = (Vec<(IssueId, LabelSnapshot)>, Vec<(IssueId, Account)>, Vec<IssueMilestone>);
