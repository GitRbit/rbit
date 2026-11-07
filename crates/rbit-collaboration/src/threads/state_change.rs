//! State transition policy, preserving SQL no-op reads and cross-feature reopen ordering.

use std::future::Future;

use rbit_contracts::{AccountId, Timestamp};

use super::{Issue, IssueId, StateReason};

/// A state change a person asks for. Closing as [`StateReason::Merged`] is refused: only
/// merging a pull request produces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChange {
    Close(StateReason),
    Reopen,
}

#[derive(Debug, Clone, Copy)]
pub struct ThreadStateAccess {
    pub can_set_state: bool,
    pub archived: bool,
}

/// Conditional mutation/event commit together. An unchanged mutation must perform
/// the original separate find, reusing its store handle; `None` means missing after
/// that read. This does not promise a snapshot spanning those separate operations.
pub trait ThreadStateRepository: Sync {
    type Error;
    fn change_state(
        &self,
        id: IssueId,
        change: StateChange,
        actor: AccountId,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<ThreadStateResult>, Self::Error>> + Send;
}

#[derive(Debug)]
pub enum ThreadStateError<E, P = E> {
    Store(E),
    Reopen(P),
    Forbidden,
    Archived,
    Invalid(String),
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadStateResult {
    pub issue: Issue,
    pub changed: bool,
}

#[derive(Debug)]
pub struct ThreadStateChanger<'a, R> {
    repository: &'a R,
    actor: AccountId,
    access: ThreadStateAccess,
}

impl<'a, R: ThreadStateRepository> ThreadStateChanger<'a, R> {
    pub fn new(repository: &'a R, actor: AccountId, access: ThreadStateAccess) -> Self {
        Self { repository, actor, access }
    }

    pub async fn set_state<P, F: Future<Output = Result<(), P>>>(
        &self,
        issue: &Issue,
        change: StateChange,
        check_reopen: impl FnOnce() -> F,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<ThreadStateResult, ThreadStateError<R::Error, P>> {
        if !self.access.can_set_state {
            return Err(ThreadStateError::Forbidden);
        }
        if self.access.archived {
            return Err(ThreadStateError::Archived);
        }
        match change {
            StateChange::Close(StateReason::Merged) => {
                return Err(ThreadStateError::Invalid("only merging a pull request closes it as merged".into()));
            }
            StateChange::Reopen if issue.is_pull() => check_reopen().await.map_err(ThreadStateError::Reopen)?,
            _ => {}
        }
        // The current database row, not the supplied snapshot, decides whether changed.
        self.repository
            .change_state(issue.id, change, self.actor, now())
            .await
            .map_err(ThreadStateError::Store)?
            .ok_or(ThreadStateError::NotFound)
    }
}
