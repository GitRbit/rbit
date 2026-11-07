//! Static milestone persistence port; SQL and row corruption checks remain unchanged.

use rbit_collaboration::{
    IssueState,
    milestones::{Milestone, MilestoneFields, MilestoneId, MilestoneProgress, MilestoneRepository},
};
use rbit_core::{RepoId, Timestamp};

use crate::{StorageError, sqlite::issue_meta::IssueMetaStore};

impl MilestoneRepository for IssueMetaStore {
    type Error = StorageError;

    async fn milestones(&self, repo: RepoId, state: Option<IssueState>) -> Result<Vec<MilestoneProgress>, Self::Error> {
        Self::milestones(self, repo, state).await
    }
    async fn milestone_counts(&self, repo: RepoId) -> Result<(u64, u64), Self::Error> {
        Self::milestone_counts(self, repo).await
    }
    async fn find_milestone_by_title(&self, repo: RepoId, title: &str) -> Result<Option<Milestone>, Self::Error> {
        Self::find_milestone_by_title(self, repo, title).await
    }
    async fn create_milestone(&self, repo: RepoId, fields: &MilestoneFields, at: Timestamp) -> Result<Milestone, Self::Error> {
        Self::create_milestone(self, repo, fields, at).await
    }
    async fn update_milestone(
        &self,
        repo: RepoId,
        id: MilestoneId,
        fields: &MilestoneFields,
        at: Timestamp,
    ) -> Result<Option<Milestone>, Self::Error> {
        Self::update_milestone(self, repo, id, fields, at).await
    }
    async fn set_milestone_state(
        &self,
        repo: RepoId,
        id: MilestoneId,
        state: IssueState,
        at: Timestamp,
    ) -> Result<Option<Milestone>, Self::Error> {
        Self::set_milestone_state(self, repo, id, state, at).await
    }
    async fn delete_milestone(&self, repo: RepoId, id: MilestoneId) -> Result<Option<Milestone>, Self::Error> {
        Self::delete_milestone(self, repo, id).await
    }
}
