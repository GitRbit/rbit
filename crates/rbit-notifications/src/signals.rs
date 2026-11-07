//! Lazy badge/read entry preserves the disabled zero-storage-work fast path.
use crate::{Subscription, port::InboxRepository};
use rbit_collaboration::threads::IssueId;
use rbit_contracts::AccountId;

pub async fn has_unread<R: InboxRepository>(enabled: bool, user: AccountId, repository: impl FnOnce() -> R) -> Result<bool, R::Error> {
    if !enabled {
        return Ok(false);
    }
    repository().has_unread(user).await
}
pub async fn mark_thread_read<R: InboxRepository>(
    enabled: bool,
    user: AccountId,
    issue: IssueId,
    repository: impl FnOnce() -> R,
) -> Result<bool, R::Error> {
    if !enabled {
        return Ok(false);
    }
    repository().mark_issue_read(user, issue).await
}
/// Subscriptions remain visible when disabled; exactly the same single store query.
pub async fn thread_state<R: InboxRepository>(
    enabled: bool,
    user: AccountId,
    issue: IssueId,
    repository: &R,
) -> Result<(Option<Subscription>, bool), R::Error> {
    let (subscription, unread) = repository.thread_state(issue, user).await?;
    Ok((subscription, unread && enabled))
}
