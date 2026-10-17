//! Domain events: an in-process publish/subscribe channel that decouples "something
//! happened" from "who reacts to it".
//!
//! Producers (the Git transports, the account service, …) publish facts; subscribers
//! (cache warming, maintenance, email, webhooks later) react independently. Publishing never
//! blocks or fails: with no subscribers an event is simply dropped, and a subscriber that
//! falls behind skips the oldest events rather than slowing producers down.
//!
//! Subscribers react *after* an event happened, sometimes noticeably later. Webhooks must
//! only see events that happened while a hook existed, so the bus also hands out webhook
//! ids ([`EventBus::reserve_webhook_id`]) and stamps every event with the newest one
//! reserved when it was published ([`Published`]): a hook with a larger id was created
//! after the event. The stamp is taken in the same atomic counter that allocates the ids,
//! so it orders events and hook creations exactly, with no clock involved.

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use rbit_core::{
    AccountId, CommentId, IssueId, IssueKind, IssueNumber, ReleaseId, RepoId, ReviewId,
    issue::{CommentAction, IssueAction, ReviewAction},
    membership::Permission,
    webhook::WebhookId,
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
    /// A repository collaborator was added, given another permission or removed, by
    /// `actor` (the collaborator themselves when they left). Published by
    /// [`crate::CollaboratorService`] (milestone 3, A).
    CollaboratorChanged { repo: RepoId, user: AccountId, actor: AccountId, change: CollaboratorChange },
}

/// What happened to a collaborator ([`DomainEvent::CollaboratorChanged`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollaboratorChange {
    Added(Permission),
    Changed { from: Permission, to: Permission },
    Removed,
}

impl CollaboratorChange {
    /// GitHub's `member` webhook action.
    pub const fn action(self) -> &'static str {
        match self {
            Self::Added(_) => "added",
            Self::Changed { .. } => "edited",
            Self::Removed => "removed",
        }
    }
}

/// An event with the point at which it was published, for subscribers that must know what
/// existed then (see [`EventBus::subscribe_published`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub event: DomainEvent,
    /// The newest webhook id reserved when the event was published. Hooks with a larger
    /// id were created after the event and do not receive it.
    pub newest_webhook: WebhookId,
}

#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<DomainEvent>,
    published: broadcast::Sender<Published>,
    /// The newest webhook id handed out (or found in the database at start-up).
    newest_webhook: Arc<AtomicI64>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            sender: broadcast::channel(CHANNEL_CAPACITY).0,
            published: broadcast::channel(CHANNEL_CAPACITY).0,
            newest_webhook: Arc::new(AtomicI64::new(0)),
        }
    }

    /// Publishes an event to every current subscriber.
    pub fn publish(&self, event: DomainEvent) {
        tracing::debug!(?event, "domain event");
        // Errors only mean there are no subscribers right now, which is fine.
        if self.published.receiver_count() > 0 {
            let _ = self.published.send(Published { event: event.clone(), newest_webhook: self.newest_webhook() });
        }
        let _ = self.sender.send(event);
    }

    /// A new subscription; it receives events published from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }

    /// A new subscription to events stamped with what existed when they were published.
    pub fn subscribe_published(&self) -> broadcast::Receiver<Published> {
        self.published.subscribe()
    }

    /// The newest webhook id reserved so far: the stamp an event published now gets.
    pub fn newest_webhook(&self) -> WebhookId {
        WebhookId(self.newest_webhook.load(Ordering::SeqCst))
    }

    /// Reserves the id of a webhook about to be created. Events published from now on carry
    /// a stamp of at least this id; events published before carry a smaller one.
    pub(crate) fn reserve_webhook_id(&self) -> WebhookId {
        WebhookId(self.newest_webhook.fetch_add(1, Ordering::SeqCst) + 1)
    }

    /// Makes sure reserved ids stay above `existing`, the newest id already stored. Called
    /// at start-up, before anything is published.
    pub(crate) fn note_existing_webhooks(&self, existing: WebhookId) {
        self.newest_webhook.fetch_max(existing.0, Ordering::SeqCst);
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

    #[tokio::test]
    async fn events_are_stamped_with_the_newest_webhook_id_reserved_before_them() {
        let bus = EventBus::new();
        bus.note_existing_webhooks(WebhookId(4));
        let mut subscriber = bus.subscribe_published();
        let event = DomainEvent::UserSignedUp { user: AccountId(1) };

        bus.publish(event.clone());
        assert_eq!(bus.reserve_webhook_id(), WebhookId(5));
        bus.publish(event.clone());
        bus.note_existing_webhooks(WebhookId(2));
        assert_eq!(bus.reserve_webhook_id(), WebhookId(6), "ids never go back");

        assert_eq!(subscriber.recv().await.unwrap(), Published { event: event.clone(), newest_webhook: WebhookId(4) });
        assert_eq!(subscriber.recv().await.unwrap(), Published { event, newest_webhook: WebhookId(5) });
    }

    #[test]
    fn publishing_without_subscribers_is_harmless() {
        EventBus::new().publish(DomainEvent::UserSignedUp { user: AccountId(1) });
    }
}
