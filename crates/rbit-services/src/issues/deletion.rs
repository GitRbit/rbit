//! Borrowed deletion effects keep audit and event policy outside the storage transaction.
use super::IssueService;
use crate::{
    DomainEvent, EventBus, IssueContext, RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};
use rbit_collaboration::issues::{IssueDeletion, IssueDeletionError, IssueDeletionPort};
use rbit_core::{Issue, IssueId, User, issue::IssueAction};
use rbit_storage::{Database, StorageError};
struct DeletionEffects<'a> {
    database: &'a Database,
    audit: &'a AuditService,
    events: &'a EventBus,
    repo: &'a RepoContext,
    actor: &'a User,
}
impl IssueDeletionPort for DeletionEffects<'_> {
    type Error = StorageError;
    async fn delete(&self, id: IssueId) -> Result<Option<Issue>, Self::Error> {
        self.database.issues().delete(id).await
    }
    async fn audit(&self, snapshot: &Issue) {
        let target = format!("{}#{}", self.repo.repo.full_name(), snapshot.number.get());
        self.audit.record(AuditEntry::new(AuditAction::IssueDeleted).by(self.actor).on(target).detail(snapshot.title.to_string())).await;
    }
    fn publish(&self, snapshot: Issue) {
        // Copy identity before moving the one returned snapshot; neither title nor body is cloned.
        self.events.publish(DomainEvent::Issue {
            repo: snapshot.repo_id,
            issue: snapshot.id,
            number: snapshot.number,
            kind: snapshot.kind,
            actor: Some(self.actor.id()),
            action: IssueAction::Deleted { snapshot: Box::new(snapshot) },
        });
    }
}
impl IssueService {
    /// Repository administrators may delete issues, never pull requests. Audited before publication.
    pub async fn delete(&self, actor: &User, ctx: &IssueContext) -> Result<(), ServiceError> {
        let effects =
            DeletionEffects { database: &self.ctx.database, audit: &self.audit, events: &self.ctx.events, repo: &ctx.repo, actor };
        IssueDeletion::new(ctx.repo.access.can_admin(), ctx.repo.repo.repo.is_archived()).delete(&effects, &ctx.issue).await.map_err(
            |error| match error {
                IssueDeletionError::Forbidden => ServiceError::Forbidden,
                IssueDeletionError::Archived => ServiceError::Archived,
                IssueDeletionError::NotFound => ServiceError::NotFound,
                IssueDeletionError::Port(error) => error.into(),
            },
        )
    }
}
