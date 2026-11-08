//! Spanish (`es`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

entries! { Es:
    TimelineEventMessage => RichWriter |text| {}
    TimelineNoteMessage => MessageWriter |text| {}
}
