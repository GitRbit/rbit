//! Existing edit entry point and legacy event translation for feature-owned policy.

use rbit_collaboration::threads::{ThreadEditAccess, ThreadEditError, ThreadEditor};
use rbit_storage::StorageError;

use super::{Issue, IssueAction, IssueContext, ServiceError, TimelineService, Timestamp, User, permissions};

fn error<P: Into<ServiceError>>(error: ThreadEditError<StorageError, P>) -> ServiceError {
    match error {
        ThreadEditError::Store(error) => error.into(),
        ThreadEditError::Throttle(error) => error.into(),
        ThreadEditError::Text(error) => error.into(),
        ThreadEditError::Forbidden => ServiceError::Forbidden,
        ThreadEditError::Archived => ServiceError::Archived,
        ThreadEditError::NotFound => ServiceError::NotFound,
    }
}

impl TimelineService {
    /// Changes title/body; unchanged values publish nothing and retain original store work.
    pub async fn edit(&self, actor: &User, ctx: &IssueContext, title: Option<&str>, body: Option<&str>) -> Result<Issue, ServiceError> {
        let access = ThreadEditAccess { can_edit: permissions(actor, ctx).can_edit, archived: ctx.repo.repo.repo.is_archived() };
        let result = ThreadEditor::new(&self.ctx.database, actor.id(), access)
            .edit(&ctx.issue, title, body, || self.spend(actor), Timestamp::now)
            .await
            .map_err(error)?;
        // As before, the store commits before transient event publication. This adds
        // no durable outbox or crash-recovery guarantee and no await before publishing.
        if let Some(change) = result.change {
            self.publish_issue(
                &result.issue,
                Some(actor.id()),
                IssueAction::Edited { previous_title: change.previous_title, previous_body: change.previous_body, previous_base: None },
            );
        }
        Ok(result.issue)
    }
}
