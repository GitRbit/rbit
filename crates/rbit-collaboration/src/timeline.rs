//! Bounded timeline pages, cursor contracts and opt-in batched reading.
mod cursor;
mod sources;
mod stored;
mod view;
pub use cursor::{EventId, TimelineCursor, TimelineItemId};
pub use sources::{IssueRef, IssueSource};
pub use stored::{StoredTimelineItem, TimelinePage};
pub use view::{CrossReferenceSource, EventSubjects, Timeline, TimelineEntry};
pub const MAX_TIMELINE_PAGE: u32 = 200;
#[cfg(feature = "applications")]
mod port;
#[cfg(feature = "applications")]
mod reader;
#[cfg(feature = "applications")]
pub use port::{CommentAccess, TimelineReadPort};
#[cfg(feature = "applications")]
pub use reader::TimelineReader;
