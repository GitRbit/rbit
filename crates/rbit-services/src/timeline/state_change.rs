//! Original state API and cross-feature reopen/event adapters.

use rbit_collaboration::threads::{ThreadStateAccess, ThreadStateChanger, ThreadStateError};
use rbit_storage::StorageError;

use super::{Issue, IssueAction, IssueContext, ServiceError, StateChange, Timestamp, User, permissions};

fn error<P: Into<ServiceError>>(error: ThreadStateError<StorageError, P>) -> ServiceError {
    match error {
        ThreadStateError::Store(error) => error.into(),
        ThreadStateError::Reopen(error) => error.into(),
        ThreadStateError::Forbidden => ServiceError::Forbidden,
        ThreadStateError::Archived => ServiceError::Archived,
        ThreadStateError::Invalid(message) => ServiceError::Invalid(message),
        ThreadStateError::NotFound => ServiceError::NotFound,
    }
}

impl crate::timeline::writer::ThreadWriter<'_> {
    /// Changes state with original permission, pull-reopen and conditional-store ordering.
    pub async fn set_state(&self, actor: &User, ctx: &IssueContext, change: StateChange) -> Result<Issue, ServiceError> {
        let access = ThreadStateAccess { can_set_state: permissions(actor, ctx).can_set_state, archived: ctx.repo.repo.repo.is_archived() };
        let result = ThreadStateChanger::new(self.database, actor.id(), access)
            .set_state(
                &ctx.issue,
                change,
                || crate::pulls::lifecycle::check_reopen_with(self.database, self.repositories, &ctx.issue),
                Timestamp::now,
            )
            .await
            .map_err(error)?;
        // Store commits before the same transient publication; no outbox guarantee.
        if result.changed {
            let action = match change {
                StateChange::Close(reason) => IssueAction::Closed { reason },
                StateChange::Reopen => IssueAction::Reopened,
            };
            self.publish_issue(&result.issue, Some(actor.id()), action);
        }
        Ok(result.issue)
    }
}
