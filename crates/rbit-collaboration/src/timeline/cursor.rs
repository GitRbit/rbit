//! Timeline cursor identity and original URL/key ordering.
use crate::comments::CommentId;
use rbit_contracts::Timestamp;
use serde::{Deserialize, Serialize};

/// Database id of a timeline event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventId(pub i64);

/// Which row a timeline item is: comments and events have separate id spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimelineItemId {
    Comment(CommentId),
    Event(EventId),
}

/// A position in a timeline: items are ordered by `(created_at, comments before events,
/// id)`, and a page starts after its cursor. Written into URLs as `{at}-c{id}` or
/// `{at}-e{id}` ([`Self::to_param`], [`Self::parse`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimelineCursor {
    pub at: Timestamp,
    pub item: TimelineItemId,
}

impl TimelineCursor {
    /// `(created_at, rank, id)` as bound to the query.
    pub fn key(self) -> (i64, i64, i64) {
        match self.item {
            TimelineItemId::Comment(id) => (self.at.0, 0, id.0),
            TimelineItemId::Event(id) => (self.at.0, 1, id.0),
        }
    }

    /// The URL form, e.g. `1760000000-c42`.
    pub fn to_param(self) -> String {
        match self.item {
            TimelineItemId::Comment(id) => format!("{}-c{}", self.at.0, id.0),
            TimelineItemId::Event(id) => format!("{}-e{}", self.at.0, id.0),
        }
    }

    /// Reads [`Self::to_param`]'s form; `None` for anything else.
    pub fn parse(value: &str) -> Option<Self> {
        let (at, item) = value.split_once('-')?;
        let at = at.parse::<i64>().ok().filter(|at| *at >= 0)?;
        let parse_id = |id: &str| id.parse::<i64>().ok().filter(|id| *id > 0);
        let item = if let Some(id) = item.strip_prefix('c') {
            TimelineItemId::Comment(CommentId(parse_id(id)?))
        } else {
            TimelineItemId::Event(EventId(parse_id(item.strip_prefix('e')?)?))
        };
        Some(Self { at: Timestamp(at), item })
    }
}
