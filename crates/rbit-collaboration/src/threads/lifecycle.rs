//! Conditional lock and privileged closure policy; repositories retain atomic mutation/event SQL.

use super::{Issue, IssueId, StateReason};
use rbit_contracts::{AccountId, Timestamp};
use std::future::Future;

pub trait ThreadLifecycleRepository: Sync {
    type Error;
    /// The caller's original event payload, without a domain-to-legacy dependency.
    type CloseEvent: Sync + ?Sized;
    fn close(
        &self,
        issue: IssueId,
        actor: Option<AccountId>,
        reason: StateReason,
        event: &Self::CloseEvent,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<Issue>, Self::Error>> + Send;
    fn change_lock(
        &self,
        issue: IssueId,
        locked: bool,
        actor: AccountId,
        at: Timestamp,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
#[derive(Debug)]
pub enum ThreadLockError<E> {
    Store(E),
    Forbidden,
    Archived,
}
#[derive(Debug, Clone, Copy)]
pub struct ThreadLocking {
    can_lock: bool,
    archived: bool,
}
impl ThreadLocking {
    pub fn new(can_lock: bool, archived: bool) -> Self {
        Self { can_lock, archived }
    }
    pub async fn set<R: ThreadLifecycleRepository>(
        &self,
        issue: IssueId,
        actor: AccountId,
        locked: bool,
        factory: impl FnOnce() -> R,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<bool, ThreadLockError<R::Error>> {
        if !self.can_lock {
            return Err(ThreadLockError::Forbidden);
        }
        if self.archived {
            return Err(ThreadLockError::Archived);
        }
        let repository = factory();
        repository.change_lock(issue, locked, actor, now()).await.map_err(ThreadLockError::Store)
    }
}
/// Privileged callers already own merge/commit authority; no user throttle/archive gate.
#[derive(Debug, Clone, Copy)]
pub struct SystemThreadClosure;
impl SystemThreadClosure {
    /// Only the conditional SQL winner returns a row. Missing/already closed stays `None`;
    /// do not add the separate fallback lookup used by the human state-change application.
    pub async fn close<R: ThreadLifecycleRepository>(
        issue: IssueId,
        actor: Option<AccountId>,
        reason: StateReason,
        event: &R::CloseEvent,
        factory: impl FnOnce() -> R,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Option<Issue>, R::Error> {
        let repository = factory();
        repository.close(issue, actor, reason, event, now()).await
    }
}
