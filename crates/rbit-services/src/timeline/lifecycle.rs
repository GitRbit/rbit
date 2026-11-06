//! Actual lock and system-closure entry points retain SQL-winner-only publication.
use super::{
    AccountId, IssueAction, IssueContext, IssueId, ServiceError, StateReason, TimelineEvent, TimelineService, Timestamp, User, permissions,
};
use rbit_collaboration::threads::{SystemThreadClosure, ThreadLockError, ThreadLocking};
use rbit_storage::StorageError;
fn error(error: ThreadLockError<StorageError>) -> ServiceError {
    match error {
        ThreadLockError::Store(error) => error.into(),
        ThreadLockError::Forbidden => ServiceError::Forbidden,
        ThreadLockError::Archived => ServiceError::Archived,
    }
}
impl TimelineService {
    /// Privileged merge/commit callers close atomically; missing/already closed publishes nothing.
    pub(crate) async fn close_as_system(
        &self,
        issue: IssueId,
        actor: Option<AccountId>,
        reason: StateReason,
        event: TimelineEvent,
    ) -> Result<bool, ServiceError> {
        let Some(updated) = SystemThreadClosure::close(issue, actor, reason, &event, || self.ctx.database.issues(), Timestamp::now).await?
        else {
            return Ok(false);
        };
        self.publish_issue(&updated, actor, IssueAction::Closed { reason });
        Ok(true)
    }
    /// Writers lock/unlock with one original conditional transaction and no content throttle.
    pub async fn set_locked(&self, actor: &User, ctx: &IssueContext, locked: bool) -> Result<(), ServiceError> {
        let changed = ThreadLocking::new(permissions(actor, ctx).can_lock, ctx.repo.repo.repo.is_archived())
            .set(ctx.issue.id, actor.id(), locked, || self.ctx.database.issues(), Timestamp::now)
            .await
            .map_err(error)?;
        if changed {
            self.publish_issue(&ctx.issue, Some(actor.id()), if locked { IssueAction::Locked } else { IssueAction::Unlocked });
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
