//! Static conditional state port; the existing SQL transaction remains authoritative.

use rbit_collaboration::threads::{IssueId, StateChange, ThreadStateRepository, ThreadStateResult};
use rbit_core::{AccountId, IssueState, TimelineEvent, Timestamp};

use crate::{Database, StorageError};

impl ThreadStateRepository for Database {
    type Error = StorageError;
    async fn change_state(
        &self,
        id: IssueId,
        change: StateChange,
        actor: AccountId,
        at: Timestamp,
    ) -> Result<Option<ThreadStateResult>, Self::Error> {
        let (state, reason, event) = match change {
            StateChange::Close(reason) => (IssueState::Closed, Some(reason), TimelineEvent::Closed { reason, commit: None, pull: None }),
            StateChange::Reopen => (IssueState::Open, None, TimelineEvent::Reopened),
        };
        let store = self.issues();
        match store.set_state(id, state, reason, Some(actor), &event, at).await? {
            Some(issue) => Ok(Some(ThreadStateResult { issue, changed: true })),
            None => Ok(store.find(id).await?.map(|issue| ThreadStateResult { issue, changed: false })),
        }
    }
}
