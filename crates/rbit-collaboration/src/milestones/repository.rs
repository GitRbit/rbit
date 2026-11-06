//! Static persistence port; adapters retain all SQL ordering and mutation semantics.

use std::future::Future;

use rbit_contracts::{RepoId, Timestamp};

use super::{Milestone, MilestoneFields, MilestoneId, MilestoneProgress};
use crate::IssueState;

pub trait MilestoneRepository: Sync {
    type Error;

    fn milestones(
        &self,
        repo: RepoId,
        state: Option<IssueState>,
    ) -> impl Future<Output = Result<Vec<MilestoneProgress>, Self::Error>> + Send;
    fn milestone_counts(&self, repo: RepoId) -> impl Future<Output = Result<(u64, u64), Self::Error>> + Send;
    fn find_milestone_by_title(&self, repo: RepoId, title: &str) -> impl Future<Output = Result<Option<Milestone>, Self::Error>> + Send;
    fn create_milestone(
        &self,
        repo: RepoId,
        fields: &MilestoneFields,
        at: Timestamp,
    ) -> impl Future<Output = Result<Milestone, Self::Error>> + Send;
    fn update_milestone(
        &self,
        repo: RepoId,
        id: MilestoneId,
        fields: &MilestoneFields,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<Milestone>, Self::Error>> + Send;
    fn set_milestone_state(
        &self,
        repo: RepoId,
        id: MilestoneId,
        state: IssueState,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<Milestone>, Self::Error>> + Send;
    fn delete_milestone(&self, repo: RepoId, id: MilestoneId) -> impl Future<Output = Result<Option<Milestone>, Self::Error>> + Send;
}
