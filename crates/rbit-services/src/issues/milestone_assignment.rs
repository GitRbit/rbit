//! One borrowed request adapter and original single store handle for assignment.

use super::{IssueService, Milestone};
use crate::{IssueContext, ServiceError};
use rbit_collaboration::milestones::{
    IssueMilestone, MilestoneAssignment, MilestoneAttachmentAccess, MilestoneAttachmentError, MilestoneAttachmentKind,
    MilestoneAttachmentPort,
};
use rbit_core::{
    MilestoneId, User,
    issue::{IssueAction, issue_permissions},
};
use rbit_storage::sqlite::issue_meta::IssueMetaStore;

struct MilestoneAssignmentView<'a> {
    service: &'a IssueService,
    actor: &'a User,
    ctx: &'a IssueContext,
    store: IssueMetaStore,
}
impl MilestoneAttachmentPort for MilestoneAssignmentView<'_> {
    type Error = ServiceError;
    async fn find(&self, id: MilestoneId) -> Result<Option<Milestone>, Self::Error> {
        Ok(self.store.find_milestone(self.ctx.repo.repo.repo.id, id).await?)
    }
    async fn current(&self) -> Result<Vec<IssueMilestone>, Self::Error> {
        Ok(self.store.milestones_of(&[self.ctx.issue.id]).await?)
    }
    async fn write(&self, id: Option<MilestoneId>) -> Result<(), Self::Error> {
        self.store.set_milestone(self.ctx.issue.id, id).await?;
        Ok(())
    }
    async fn record(&self, kind: MilestoneAttachmentKind, id: MilestoneId, title: &str) -> Result<(), Self::Error> {
        let event = match kind {
            MilestoneAttachmentKind::Removed => rbit_core::TimelineEvent::Demilestoned { milestone: id, title: title.to_owned() },
            MilestoneAttachmentKind::Added => rbit_core::TimelineEvent::Milestoned { milestone: id, title: title.to_owned() },
        };
        self.service.timeline.writer().record(self.ctx.issue.id, Some(self.actor.id()), event).await?;
        Ok(())
    }
    fn publish(&self, kind: MilestoneAttachmentKind, id: MilestoneId, title: String) {
        let action = match kind {
            MilestoneAttachmentKind::Removed => IssueAction::Demilestoned { milestone: id, title },
            MilestoneAttachmentKind::Added => IssueAction::Milestoned { milestone: id },
        };
        self.service.publish(&self.ctx.issue, Some(self.actor.id()), action);
    }
}
fn error(error: MilestoneAttachmentError<ServiceError>) -> ServiceError {
    match error {
        MilestoneAttachmentError::Port(error) | MilestoneAttachmentError::Throttle(error) => error,
        MilestoneAttachmentError::Forbidden => ServiceError::Forbidden,
        MilestoneAttachmentError::Archived => ServiceError::Archived,
        MilestoneAttachmentError::Invalid(message) => ServiceError::Invalid(message),
    }
}
impl IssueService {
    /// Sets the milestone, retaining same-assignment fast path and history ordering.
    pub async fn set_milestone(&self, actor: &User, ctx: &IssueContext, milestone: Option<MilestoneId>) -> Result<(), ServiceError> {
        let access = MilestoneAttachmentAccess {
            can_triage: issue_permissions(Some(actor), ctx.repo.access, &ctx.issue).can_triage,
            archived: ctx.repo.repo.repo.is_archived(),
        };
        MilestoneAssignment::new(access)
            .set(
                milestone,
                || MilestoneAssignmentView { service: self, actor, ctx, store: self.ctx.database.issue_meta() },
                || self.spend(actor),
            )
            .await
            .map_err(error)
    }
}
