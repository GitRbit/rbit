//! Request-local assignee storage and cross-feature active-user/grant/event adapters.

use super::IssueService;
use crate::{IssueContext, ServiceError};
use rbit_collaboration::assignees::{AssigneeAccess, AssigneeAssignments, AssigneeAttachmentKind, AssigneeAttachmentPort, AssigneeError};
use rbit_core::{
    Account, AccountId, IssueId, Timestamp, User,
    issue::{IssueAction, issue_permissions},
};
use rbit_storage::sqlite::issue_meta::IssueMetaStore;

struct AssigneeAttachmentView<'a> {
    service: &'a IssueService,
    actor: &'a User,
    ctx: &'a IssueContext,
    store: IssueMetaStore,
    /// Taking: assign only while nobody is assigned.
    only_if_free: bool,
}
impl AssigneeAttachmentPort for AssigneeAttachmentView<'_> {
    type Error = ServiceError;
    async fn current(&self) -> Result<Vec<(IssueId, Account)>, Self::Error> {
        Ok(self.store.assignees_of(&[self.ctx.issue.id]).await?)
    }
    async fn resolve_new(&self, ids: &[AccountId]) -> Result<Vec<User>, Self::Error> {
        self.service.validated_assignees(&self.ctx.repo.repo, ids).await
    }
    async fn remove(&self, id: AccountId) -> Result<bool, Self::Error> {
        Ok(self.store.remove_assignee(self.ctx.issue.id, id).await?)
    }
    async fn add(&self, id: AccountId, at: Timestamp) -> Result<bool, Self::Error> {
        if self.only_if_free {
            return Ok(self.store.add_assignee_if_unassigned(self.ctx.issue.id, id, self.actor.id(), at).await?);
        }
        Ok(self.store.add_assignee(self.ctx.issue.id, id, self.actor.id(), at).await?)
    }
    async fn record(&self, kind: AssigneeAttachmentKind, id: AccountId) -> Result<(), Self::Error> {
        let event = match kind {
            AssigneeAttachmentKind::Assigned => rbit_core::TimelineEvent::Assigned { assignee: id },
            AssigneeAttachmentKind::Unassigned => rbit_core::TimelineEvent::Unassigned { assignee: id },
        };
        self.service.timeline.writer().record(self.ctx.issue.id, Some(self.actor.id()), event).await?;
        Ok(())
    }
    fn publish(&self, kind: AssigneeAttachmentKind, id: AccountId) {
        let action = match kind {
            AssigneeAttachmentKind::Assigned => IssueAction::Assigned { assignee: id },
            AssigneeAttachmentKind::Unassigned => IssueAction::Unassigned { assignee: id },
        };
        self.service.publish(&self.ctx.issue, Some(self.actor.id()), action);
    }
}
fn access(actor: &User, ctx: &IssueContext) -> AssigneeAccess {
    AssigneeAccess {
        can_triage: issue_permissions(Some(actor), ctx.repo.access, &ctx.issue).can_triage,
        archived: ctx.repo.repo.repo.is_archived(),
    }
}
fn error(error: AssigneeError<ServiceError>) -> ServiceError {
    match error {
        AssigneeError::Port(error) | AssigneeError::Throttle(error) => error,
        AssigneeError::Forbidden => ServiceError::Forbidden,
        AssigneeError::Archived => ServiceError::Archived,
        AssigneeError::Invalid(message) => ServiceError::Invalid(message),
    }
}
impl IssueService {
    /// Adds/removes assignees, validating only new IDs and preserving add-wins semantics.
    pub async fn change_assignees(
        &self,
        actor: &User,
        ctx: &IssueContext,
        add: &[AccountId],
        remove: &[AccountId],
    ) -> Result<(), ServiceError> {
        AssigneeAssignments::new(access(actor, ctx))
            .change(
                add,
                remove,
                || AssigneeAttachmentView { service: self, actor, ctx, store: self.ctx.database.issue_meta(), only_if_free: false },
                || self.spend(actor),
                Timestamp::now,
            )
            .await
            .map_err(error)
    }
    /// Replaces assignees with the existing initial-read/delegated-reread behavior.
    pub async fn set_assignees(&self, actor: &User, ctx: &IssueContext, assignees: &[AccountId]) -> Result<(), ServiceError> {
        AssigneeAssignments::new(access(actor, ctx))
            .set(
                assignees,
                || AssigneeAttachmentView { service: self, actor, ctx, store: self.ctx.database.issue_meta(), only_if_free: false },
                || self.spend(actor),
                Timestamp::now,
            )
            .await
            .map_err(error)
    }

    // Agent loop: taking an issue.
    /// Takes the issue for `actor`: assigns them only while nobody holds it, as one
    /// conditional write. `Ok(None)` when it is now theirs (or already was); otherwise the
    /// login of who holds it.
    pub async fn take(&self, actor: &User, ctx: &IssueContext) -> Result<Option<String>, ServiceError> {
        AssigneeAssignments::new(access(actor, ctx))
            .change(
                &[actor.id()],
                &[],
                || AssigneeAttachmentView { service: self, actor, ctx, store: self.ctx.database.issue_meta(), only_if_free: true },
                || self.spend(actor),
                Timestamp::now,
            )
            .await
            .map_err(error)?;
        let holders = self.ctx.database.issue_meta().assignees_of(&[ctx.issue.id]).await?;
        if holders.iter().any(|(_, account)| account.id == actor.id()) {
            return Ok(None);
        }
        Ok(Some(holders.first().map_or_else(|| "someone".to_owned(), |(_, account)| account.name.to_string())))
    }
}
