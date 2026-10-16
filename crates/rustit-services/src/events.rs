//! Domain events: an in-process publish/subscribe channel that decouples "something
//! happened" from "who reacts to it".
//!
//! Producers (the Git transports, the account service, …) publish facts; subscribers
//! (cache warming, maintenance, email, webhooks later) react independently. Publishing never
//! blocks or fails: with no subscribers an event is simply dropped, and a subscriber that
//! falls behind skips the oldest events rather than slowing producers down.

use rustit_core::{AccountId, RepoId};
use tokio::sync::broadcast;

/// Events buffered per subscriber before the oldest are skipped.
const CHANNEL_CAPACITY: usize = 1024;

/// Something that happened in the domain. Variants carry ids, not loaded entities, so
/// subscribers always read current state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEvent {
    /// New objects or ref updates were accepted by a push (HTTP or SSH).
    RepositoryPushed { repo: RepoId },
    /// An account was created, by sign-up or by an administrator.
    UserSignedUp { user: AccountId },
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
        bus.publish(DomainEvent::RepositoryPushed { repo: RepoId(7) });
        assert_eq!(subscriber.recv().await.unwrap(), DomainEvent::RepositoryPushed { repo: RepoId(7) });
    }

    #[test]
    fn publishing_without_subscribers_is_harmless() {
        EventBus::new().publish(DomainEvent::RepositoryPushed { repo: RepoId(1) });
    }
}
