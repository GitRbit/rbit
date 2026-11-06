//! Existing milestone service paths delegate catalog authority to collaboration.

use rbit_collaboration::milestones::{MilestoneAccess, MilestoneApplication, MilestoneError};
use rbit_core::{IssueState, MilestoneId, Timestamp, User};
use rbit_storage::{StorageError, sqlite::issue_meta::IssueMetaStore};

use super::{IssueService, Milestone, MilestoneInput, MilestoneProgress};
use crate::{RepoContext, ServiceError};

fn access(repo: &RepoContext) -> MilestoneAccess {
    MilestoneAccess { repo: repo.repo.repo.id, can_write: repo.access.can_write(), archived: repo.repo.repo.is_archived() }
}

fn error<P: Into<ServiceError>>(error: MilestoneError<StorageError, P>) -> ServiceError {
    match error {
        MilestoneError::Store(error) => error.into(),
        MilestoneError::Throttle(error) => error.into(),
        MilestoneError::Invalid(message) => ServiceError::Invalid(message),
        MilestoneError::Forbidden => ServiceError::Forbidden,
        MilestoneError::Archived => ServiceError::Archived,
        MilestoneError::NotFound => ServiceError::NotFound,
    }
}

impl IssueService {
    fn milestone_store(&self) -> IssueMetaStore {
        self.ctx.database.issue_meta()
    }

    /// The milestones of `repo` (in `state` if given) with their progress.
    pub async fn milestones(&self, repo: &RepoContext, state: Option<IssueState>) -> Result<Vec<MilestoneProgress>, ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo)).list(state).await.map_err(error)
    }

    /// How many milestones of `repo` are open and closed.
    pub async fn milestone_counts(&self, repo: &RepoContext) -> Result<(u64, u64), ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo)).counts().await.map_err(error)
    }

    /// Milestone `id` of `repo` with its progress.
    pub async fn milestone(&self, repo: &RepoContext, id: MilestoneId) -> Result<MilestoneProgress, ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo)).find(id).await.map_err(error)
    }

    /// The milestone of `repo` titled `title` (case-insensitively).
    pub async fn milestone_titled(&self, repo: &RepoContext, title: &str) -> Result<Milestone, ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo)).titled(title).await.map_err(error)
    }

    /// Creates an open milestone (writers).
    pub async fn create_milestone(&self, actor: &User, repo: &RepoContext, input: &MilestoneInput) -> Result<Milestone, ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo))
            .create(input, || self.spend(actor), Timestamp::now)
            .await
            .map_err(error)
    }

    /// Changes milestone `id` of `repo` (writers).
    pub async fn update_milestone(
        &self,
        actor: &User,
        repo: &RepoContext,
        id: MilestoneId,
        input: &MilestoneInput,
    ) -> Result<Milestone, ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo))
            .update(id, input, || self.spend(actor), Timestamp::now)
            .await
            .map_err(error)
    }

    /// Closes or reopens milestone `id` of `repo` (writers).
    pub async fn set_milestone_state(&self, repo: &RepoContext, id: MilestoneId, state: IssueState) -> Result<Milestone, ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo)).set_state(id, state, Timestamp::now).await.map_err(error)
    }

    /// Deletes milestone `id` of `repo` (writers); its threads keep no milestone.
    pub async fn delete_milestone(&self, repo: &RepoContext, id: MilestoneId) -> Result<Milestone, ServiceError> {
        MilestoneApplication::new(&self.milestone_store(), access(repo)).delete(id).await.map_err(error)
    }
}
