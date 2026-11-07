//! Static service bridges preserve cross-feature grants and timeline/event adapters.

use std::future::Future;

use rbit_contracts::AccountId;
use rbit_identity_models::User;

use crate::{
    labels::{Label, LabelId, LabelSnapshot},
    milestones::{Milestone, MilestoneId},
    threads::{Issue, IssueId},
};

#[derive(Debug)]
pub enum MetadataCreated {
    Labeled(LabelSnapshot),
    Assigned(AccountId),
    Milestoned { id: MilestoneId, title: String },
}

/// Metadata validation preserves ordered per-ID lookups and grant checks.
/// Row creation, attachment and each event remain separate commits, not an outbox.
pub trait IssueCreationPort: Sync {
    type Error;
    fn labels(&self, ids: &[LabelId]) -> impl Future<Output = Result<Vec<Label>, Self::Error>> + Send;
    fn assignees(&self, ids: &[AccountId]) -> impl Future<Output = Result<Vec<User>, Self::Error>> + Send;
    fn milestone(&self, id: MilestoneId) -> impl Future<Output = Result<Option<Milestone>, Self::Error>> + Send;
    fn create_thread(&self, title: &str, body: &str) -> impl Future<Output = Result<Issue, Self::Error>> + Send;
    fn attach(
        &self,
        issue: IssueId,
        labels: &[LabelId],
        assignees: &[AccountId],
        milestone: Option<MilestoneId>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn record(&self, issue: IssueId, event: MetadataCreated) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn publish_opened(&self, issue: &Issue);
}
