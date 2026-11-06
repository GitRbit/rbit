//! Timeline presentation records keep event payloads statically typed.
use super::{EventId, TimelineCursor, TimelineItemId};
use crate::{
    IssueState,
    comments::Comment,
    threads::{IssueKind, IssueNumber},
};
use rbit_contracts::Timestamp;
use rbit_identity_models::Account;

/// One item of a thread's conversation, shaped for a viewer.
#[derive(Debug, Clone)]
pub enum TimelineEntry<E> {
    Comment {
        comment: Comment,
        /// `None` once the account is deleted (a ghost).
        author: Option<Account>,
        can_edit: bool,
        can_delete: bool,
    },
    Event {
        id: EventId,
        /// `None` for system actions and deleted accounts.
        actor: Option<Account>,
        event: E,
        at: Timestamp,
        subjects: EventSubjects,
    },
}

impl<E> TimelineEntry<E> {
    /// Where the entry sits in the timeline (for interleaving other items, such as reviews).
    pub fn cursor(&self) -> TimelineCursor {
        match self {
            Self::Comment { comment, .. } => TimelineCursor { at: comment.created_at, item: TimelineItemId::Comment(comment.id) },
            Self::Event { id, at, .. } => TimelineCursor { at: *at, item: TimelineItemId::Event(*id) },
        }
    }
}

/// What an event names besides its actor: the account (assignee, reviewer; `None` when
/// deleted or not applicable) and, for `CrossReferenced`, the source thread.
#[derive(Debug, Clone, Default)]
pub struct EventSubjects {
    pub account: Option<Account>,
    pub source: Option<CrossReferenceSource>,
}

/// The thread a cross-reference comes from, which the viewer may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossReferenceSource {
    /// `owner/name` of its repository.
    pub full_name: String,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub state: IssueState,
    pub title: String,
}

/// One page of a timeline; `next` continues it.
#[derive(Debug, Clone)]
pub struct Timeline<E> {
    pub entries: Vec<TimelineEntry<E>>,
    pub next: Option<TimelineCursor>,
}

impl<E> Default for Timeline<E> {
    fn default() -> Self {
        Self { entries: Vec::new(), next: None }
    }
}
