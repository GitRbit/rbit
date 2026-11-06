//! Forward the original conditional SQL and transaction-bound timeline events.
use crate::{StorageError, sqlite::issues::IssueStore};
use rbit_collaboration::threads::{Issue, IssueId, StateReason, ThreadLifecycleRepository};
use rbit_core::{AccountId, IssueState, TimelineEvent, Timestamp};
impl ThreadLifecycleRepository for IssueStore {
    type Error = StorageError;
    type CloseEvent = TimelineEvent;
    async fn close(
        &self,
        issue: IssueId,
        actor: Option<AccountId>,
        reason: StateReason,
        event: &Self::CloseEvent,
        at: Timestamp,
    ) -> Result<Option<Issue>, Self::Error> {
        self.set_state(issue, IssueState::Closed, Some(reason), actor, event, at).await
    }
    async fn change_lock(&self, issue: IssueId, locked: bool, actor: AccountId, at: Timestamp) -> Result<bool, Self::Error> {
        self.set_locked(issue, locked, actor, at).await
    }
}
