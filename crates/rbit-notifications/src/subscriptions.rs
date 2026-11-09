//! Manual thread subscriptions. Both remain available while notifications are off.
use crate::{Subscription, port::InboxRepository};
use rbit_collaboration::threads::IssueId;
use rbit_contracts::{AccountId, Timestamp};

/// `user`'s subscription to `issue`, if they have one.
pub async fn subscription<R: InboxRepository>(repository: &R, user: AccountId, issue: IssueId) -> Result<Option<Subscription>, R::Error> {
    repository.subscription(issue, user).await
}

/// Subscribes or unsubscribes `user` by hand. Subscribing keeps a stronger existing
/// reason; unsubscribing sticks until they subscribe again, even against participation.
pub async fn set_subscribed<R: InboxRepository>(
    repository: &R,
    user: AccountId,
    issue: IssueId,
    subscribed: bool,
    now: Timestamp,
) -> Result<(), R::Error> {
    repository.set_subscribed(issue, user, subscribed, now).await
}
