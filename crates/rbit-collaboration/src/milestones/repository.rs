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

// Borrowed factories retain repository identity without cloning the handle.
impl<R: MilestoneRepository + ?Sized> MilestoneRepository for &R {
    type Error = R::Error;

    async fn milestones(&self, repo: RepoId, state: Option<IssueState>) -> Result<Vec<MilestoneProgress>, Self::Error> {
        R::milestones(*self, repo, state).await
    }
    async fn milestone_counts(&self, repo: RepoId) -> Result<(u64, u64), Self::Error> {
        R::milestone_counts(*self, repo).await
    }
    async fn find_milestone_by_title(&self, repo: RepoId, title: &str) -> Result<Option<Milestone>, Self::Error> {
        R::find_milestone_by_title(*self, repo, title).await
    }
    async fn create_milestone(&self, repo: RepoId, fields: &MilestoneFields, at: Timestamp) -> Result<Milestone, Self::Error> {
        R::create_milestone(*self, repo, fields, at).await
    }
    async fn update_milestone(
        &self,
        repo: RepoId,
        id: MilestoneId,
        fields: &MilestoneFields,
        at: Timestamp,
    ) -> Result<Option<Milestone>, Self::Error> {
        R::update_milestone(*self, repo, id, fields, at).await
    }
    async fn set_milestone_state(
        &self,
        repo: RepoId,
        id: MilestoneId,
        state: IssueState,
        at: Timestamp,
    ) -> Result<Option<Milestone>, Self::Error> {
        R::set_milestone_state(*self, repo, id, state, at).await
    }
    async fn delete_milestone(&self, repo: RepoId, id: MilestoneId) -> Result<Option<Milestone>, Self::Error> {
        R::delete_milestone(*self, repo, id).await
    }
}
