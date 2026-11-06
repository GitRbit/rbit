//! Request-local static bridges for label assignment and ordered timeline/publication.

use super::{IssueService, Label};
use crate::{IssueContext, ServiceError};
use rbit_collaboration::labels::{
    LabelAttachmentAccess, LabelAttachmentError, LabelAttachmentKind, LabelAttachmentMutation, LabelAttachmentPort, LabelAttachments,
};
use rbit_core::{
    IssueId, LabelId, Timestamp, User,
    issue::{IssueAction, LabelSnapshot, issue_permissions},
};
use rbit_storage::sqlite::issue_meta::IssueMetaStore;

struct LabelAttachmentContext<'a> {
    service: &'a IssueService,
    actor: &'a User,
    ctx: &'a IssueContext,
}
struct LabelAttachmentMutationView<'a> {
    service: &'a IssueService,
    actor: &'a User,
    ctx: &'a IssueContext,
    store: IssueMetaStore,
}

impl<'a> LabelAttachmentPort for LabelAttachmentContext<'a> {
    type Error = ServiceError;
    type Mutation = LabelAttachmentMutationView<'a>;
    async fn resolve(&self, ids: &[LabelId]) -> Result<Vec<Label>, Self::Error> {
        self.service.repo_labels(self.ctx.repo.repo.repo.id, ids).await
    }
    fn mutation(&self) -> Self::Mutation {
        LabelAttachmentMutationView {
            service: self.service,
            actor: self.actor,
            ctx: self.ctx,
            store: self.service.ctx.database.issue_meta(),
        }
    }
}
impl LabelAttachmentMutation for LabelAttachmentMutationView<'_> {
    type Error = ServiceError;
    async fn current(&self) -> Result<Vec<(IssueId, LabelSnapshot)>, Self::Error> {
        Ok(self.store.labels_of(&[self.ctx.issue.id]).await?)
    }
    async fn remove(&self, id: LabelId) -> Result<bool, Self::Error> {
        Ok(self.store.remove_label(self.ctx.issue.id, id).await?)
    }
    async fn add(&self, id: LabelId, at: Timestamp) -> Result<bool, Self::Error> {
        Ok(self.store.add_label(self.ctx.issue.id, id, at).await?)
    }
    async fn record(&self, kind: LabelAttachmentKind, label: &LabelSnapshot) -> Result<(), Self::Error> {
        let event = match kind {
            LabelAttachmentKind::Labeled => rbit_core::TimelineEvent::Labeled { label: label.clone() },
            LabelAttachmentKind::Unlabeled => rbit_core::TimelineEvent::Unlabeled { label: label.clone() },
        };
        self.service.timeline.record(self.ctx.issue.id, Some(self.actor.id()), event).await?;
        Ok(())
    }
    fn publish(&self, kind: LabelAttachmentKind, label: LabelSnapshot) {
        let action = match kind {
            LabelAttachmentKind::Labeled => IssueAction::Labeled { label },
            LabelAttachmentKind::Unlabeled => IssueAction::Unlabeled { label },
        };
        self.service.publish(&self.ctx.issue, Some(self.actor.id()), action);
    }
}
fn access(actor: &User, ctx: &IssueContext) -> LabelAttachmentAccess {
    LabelAttachmentAccess {
        can_triage: issue_permissions(Some(actor), ctx.repo.access, &ctx.issue).can_triage,
        archived: ctx.repo.repo.repo.is_archived(),
    }
}
fn error(error: LabelAttachmentError<ServiceError>) -> ServiceError {
    match error {
        LabelAttachmentError::Port(error) | LabelAttachmentError::Throttle(error) => error,
        LabelAttachmentError::Forbidden => ServiceError::Forbidden,
        LabelAttachmentError::Archived => ServiceError::Archived,
        LabelAttachmentError::Invalid(message) => ServiceError::Invalid(message),
    }
}
impl IssueService {
    /// Changes labels with duplicate suppression and additions winning over removals.
    pub async fn change_labels(&self, actor: &User, ctx: &IssueContext, add: &[LabelId], remove: &[LabelId]) -> Result<(), ServiceError> {
        let bridge = LabelAttachmentContext { service: self, actor, ctx };
        LabelAttachments::new(&bridge, access(actor, ctx)).change(add, remove, || self.spend(actor), Timestamp::now).await.map_err(error)
    }
    /// Makes labels exact, retaining the existing initial read and delegated reread.
    pub async fn set_labels(&self, actor: &User, ctx: &IssueContext, labels: &[LabelId]) -> Result<(), ServiceError> {
        let bridge = LabelAttachmentContext { service: self, actor, ctx };
        LabelAttachments::new(&bridge, access(actor, ctx)).set(labels, || self.spend(actor), Timestamp::now).await.map_err(error)
    }
}
