//! Domain events: an in-process publish/subscribe channel that decouples "something
//! happened" from "who reacts to it".
//!
//! Producers (the Git transports, the account service, …) publish facts; subscribers
//! (cache warming, maintenance, email, webhooks later) react independently. Publishing never
//! blocks or fails: with no subscribers an event is simply dropped, and a subscriber that
//! falls behind skips the oldest events rather than slowing producers down.

use rbit_core::{
    AccountId, CommentId, IssueId, IssueKind, IssueNumber, ReleaseId, RepoId, ReviewId,
    issue::{CommentAction, IssueAction, ReviewAction},
};
pub use rbit_git::RefUpdate;
use tokio::sync::broadcast;

/// Events buffered per subscriber before the oldest are skipped.
const CHANNEL_CAPACITY: usize = 1024;

/// Something that happened in the domain. Variants carry ids, not loaded entities, so
/// subscribers always read current state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEvent {
    /// A push (HTTP or SSH) was accepted. Published by [`crate::PushService::complete`].
    RepositoryPushed {
        repo: RepoId,
        /// The account that pushed; `None` for deploy keys.
        pusher: Option<AccountId>,
        /// The refs the push changed, ordered by name. Empty when nothing changed (or the
        /// refs could not be read; that is logged).
        updates: Vec<RefUpdate>,
    },
    /// A repository was created (not by forking; see [`Self::RepositoryForked`]).
    RepositoryCreated { repo: RepoId, actor: AccountId },
    /// A repository was deleted, by an administrator of it or of the site. The row is
    /// gone, so the event carries what subscribers still need to describe it.
    RepositoryDeleted {
        repo: RepoId,
        owner: AccountId,
        /// `owner/name` at the time of deletion.
        full_name: String,
        /// The user who deleted it (for an account deletion, the site administrator).
        actor: AccountId,
    },
    /// A repository moved to another owner, by `actor`.
    RepositoryTransferred { repo: RepoId, from: AccountId, to: AccountId, actor: AccountId },
    /// `repo` was created as a fork of `parent`. Published by the repository essentials
    /// workstream.
    RepositoryForked { repo: RepoId, parent: RepoId, actor: AccountId },
    /// A release was published (created as non-draft, or a draft was published).
    /// Published by the repository essentials workstream.
    ReleasePublished { repo: RepoId, release: ReleaseId, actor: AccountId },
    /// An account was created, by sign-up or by an administrator.
    UserSignedUp { user: AccountId },
    /// A user's password was changed (by them, by a reset link, or by an administrator).
    PasswordChanged { user: AccountId },
    /// A user's email address was changed; `previous` is the old address.
    EmailChanged { user: AccountId, previous: String },
    /// Something happened to an issue or pull request (`kind` tells which). Published by
    /// the timeline (T0) and the issue (I) and pull request (P) services.
    Issue {
        repo: RepoId,
        issue: IssueId,
        number: IssueNumber,
        kind: IssueKind,
        /// `None` for system actions: a deploy-key push or a mirror fetch closing an issue.
        actor: Option<AccountId>,
        action: IssueAction,
    },
    /// A comment on an issue or pull request was created, edited or deleted.
    IssueComment {
        repo: RepoId,
        issue: IssueId,
        number: IssueNumber,
        kind: IssueKind,
        comment: CommentId,
        actor: AccountId,
        action: CommentAction,
    },
    /// A pull request review was submitted, edited or dismissed. Published by P.
    PullRequestReview { repo: RepoId, pull: IssueId, number: IssueNumber, review: ReviewId, actor: AccountId, action: ReviewAction },
}

#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<DomainEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self { sender: broadcast::channel(CHANNEL_CAPACITY).0 }
    }

    /// Publishes an event to every current subscriber.
    pub fn publish(&self, event: DomainEvent) {
        tracing::debug!(?event, "domain event");
        // An error only means there are no subscribers right now, which is fine.
        let _ = self.sender.send(event);
    }

    /// A new subscription; it receives events published from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_receive_events_published_after_subscribing() {
        let bus = EventBus::new();
        bus.publish(DomainEvent::UserSignedUp { user: AccountId(1) });

        let mut subscriber = bus.subscribe();
        let pushed = DomainEvent::RepositoryPushed { repo: RepoId(7), pusher: Some(AccountId(1)), updates: Vec::new() };
        bus.publish(pushed.clone());
        assert_eq!(subscriber.recv().await.unwrap(), pushed);
    }

    #[test]
    fn publishing_without_subscribers_is_harmless() {
        EventBus::new().publish(DomainEvent::UserSignedUp { user: AccountId(1) });
    }
}
