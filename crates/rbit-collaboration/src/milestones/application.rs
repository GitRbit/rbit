//! Milestone catalog policy; no thread attachment, timeline or audit side effects.

use rbit_contracts::{RepoId, Timestamp};

use super::{Milestone, MilestoneId, MilestoneInput, MilestoneProgress, MilestoneRepository, milestone_fields};
use crate::IssueState;

#[derive(Debug, Clone, Copy)]
pub struct MilestoneAccess {
    pub repo: RepoId,
    pub can_write: bool,
    pub archived: bool,
}

impl MilestoneAccess {
    fn require_writer<E, P>(self) -> Result<(), MilestoneError<E, P>> {
        if !self.can_write {
            return Err(MilestoneError::Forbidden);
        }
        if self.archived {
            return Err(MilestoneError::Archived);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum MilestoneError<E, P = E> {
    Store(E),
    Throttle(P),
    Invalid(String),
    Forbidden,
    Archived,
    NotFound,
}

/// Statically composed catalog policy using the caller's already-resolved access.
#[derive(Debug)]
pub struct MilestoneApplication<'a, R> {
    repository: &'a R,
    access: MilestoneAccess,
}

impl<'a, R: MilestoneRepository> MilestoneApplication<'a, R> {
    pub fn new(repository: &'a R, access: MilestoneAccess) -> Self {
        Self { repository, access }
    }

    pub async fn list(&self, state: Option<IssueState>) -> Result<Vec<MilestoneProgress>, MilestoneError<R::Error>> {
        self.repository.milestones(self.access.repo, state).await.map_err(MilestoneError::Store)
    }

    pub async fn counts(&self) -> Result<(u64, u64), MilestoneError<R::Error>> {
        self.repository.milestone_counts(self.access.repo).await.map_err(MilestoneError::Store)
    }

    pub async fn find(&self, id: MilestoneId) -> Result<MilestoneProgress, MilestoneError<R::Error>> {
        // Keep the existing single list-with-progress statement, not a new ID lookup.
        self.list(None).await?.into_iter().find(|progress| progress.milestone.id == id).ok_or(MilestoneError::NotFound)
    }

    pub async fn titled(&self, title: &str) -> Result<Milestone, MilestoneError<R::Error>> {
        self.repository
            .find_milestone_by_title(self.access.repo, title)
            .await
            .map_err(MilestoneError::Store)?
            .ok_or(MilestoneError::NotFound)
    }

    pub async fn create<P>(
        &self,
        input: &MilestoneInput,
        spend: impl FnOnce() -> Result<(), P>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Milestone, MilestoneError<R::Error, P>> {
        self.access.require_writer()?;
        let fields = milestone_fields(input).map_err(MilestoneError::Invalid)?;
        spend().map_err(MilestoneError::Throttle)?;
        self.repository.create_milestone(self.access.repo, &fields, now()).await.map_err(MilestoneError::Store)
    }

    pub async fn update<P>(
        &self,
        id: MilestoneId,
        input: &MilestoneInput,
        spend: impl FnOnce() -> Result<(), P>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Milestone, MilestoneError<R::Error, P>> {
        self.access.require_writer()?;
        let fields = milestone_fields(input).map_err(MilestoneError::Invalid)?;
        spend().map_err(MilestoneError::Throttle)?;
        self.repository
            .update_milestone(self.access.repo, id, &fields, now())
            .await
            .map_err(MilestoneError::Store)?
            .ok_or(MilestoneError::NotFound)
    }

    /// State changes retain their original lack of content-throttle charges.
    pub async fn set_state(
        &self,
        id: MilestoneId,
        state: IssueState,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Milestone, MilestoneError<R::Error>> {
        self.access.require_writer()?;
        self.repository
            .set_milestone_state(self.access.repo, id, state, now())
            .await
            .map_err(MilestoneError::Store)?
            .ok_or(MilestoneError::NotFound)
    }

    pub async fn delete(&self, id: MilestoneId) -> Result<Milestone, MilestoneError<R::Error>> {
        self.access.require_writer()?;
        self.repository.delete_milestone(self.access.repo, id).await.map_err(MilestoneError::Store)?.ok_or(MilestoneError::NotFound)
    }
}
