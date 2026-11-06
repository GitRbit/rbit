//! Stored page records keep generic event payloads without a legacy-core dependency.
use super::{EventId, TimelineCursor, TimelineItemId};
use crate::comments::Comment;
use rbit_contracts::{AccountId, Timestamp};
use rbit_identity_models::Account;

/// A comment or event as stored, with its author or actor account (`None`: deleted).
#[derive(Debug, Clone)]
pub enum StoredTimelineItem<E> {
    Comment {
        comment: Comment,
        author: Option<Account>,
    },
    Event {
        id: EventId,
        actor: Option<Account>,
        /// The actor's id even when the account row is gone (it is `NULL` then too, by
        /// `ON DELETE SET NULL`, so this is `None` exactly when `actor` is).
        actor_id: Option<AccountId>,
        event: E,
        at: Timestamp,
    },
}

impl<E> StoredTimelineItem<E> {
    pub fn cursor(&self) -> TimelineCursor {
        match self {
            Self::Comment { comment, .. } => TimelineCursor { at: comment.created_at, item: TimelineItemId::Comment(comment.id) },
            Self::Event { id, at, .. } => TimelineCursor { at: *at, item: TimelineItemId::Event(*id) },
        }
    }
}

/// One page of a timeline. `next` is set when more items follow; events of unknown kinds
/// are left out of `items` but still count towards the page.
#[derive(Debug, Clone)]
pub struct TimelinePage<E> {
    pub items: Vec<StoredTimelineItem<E>>,
    pub next: Option<TimelineCursor>,
}

impl<E> Default for TimelinePage<E> {
    fn default() -> Self {
        Self { items: Vec::new(), next: None }
    }
}
