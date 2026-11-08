//! Issues, pull requests, reviews, labels, milestones and their timelines.
//!
//! Every family here is English-only so far: another locale falls back to English through
//! [`super::fallback`], and templates mark that text `lang="en"`. Plain families implement
//! [`super::fallback::MessageFamily`]; sentences that embed markup (a link, a time, a branch) implement
//! [`RichFamily`] and name each embedded piece as a numbered slot, so a translation may
//! reorder them. Templates look a message up on its type: `IssueMessage::NewIssue
//! .localized(locale)` or `CollabSentence::OpenCount { count }.rich(locale)`.
use std::borrow::Borrow;

use super::{Catalog, fallback::Localized};
use crate::locale::Locale;

mod events;
mod filters;
mod issues;
mod labels;
mod merge;
mod milestones;
mod pulls;
mod review;
mod rich;
mod sentences;
mod sidebar;
mod thread;

pub use events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
pub use filters::{IssueFilterMenu, IssueFilterMessage};
pub use issues::IssueMessage;
pub use labels::LabelMessage;
pub use merge::MergeMessage;
pub use milestones::MilestoneMessage;
pub use pulls::PullMessage;
pub use review::ReviewMessage;
pub use rich::{Rich, RichFamily, RichPart, RichWriter};
pub use sentences::CollabSentence;
pub use sidebar::IssueSidebarMessage;
pub use thread::ThreadMessage;

/// Adds `localized(locale)` to plain families: the lookup templates call.
macro_rules! plain_lookup {
    ($($family:ident $(<$lifetime:lifetime>)?),* $(,)?) => {$(
        impl $family $(<$lifetime>)? {
            /// This message in `locale`, or its English text marked as a fallback.
            pub fn localized(self, locale: impl Borrow<Locale>) -> Localized {
                Catalog::new(*locale.borrow()).localized(self)
            }
        }
    )*};
}

/// Adds `rich(locale)` to rich families.
macro_rules! rich_lookup {
    ($($family:ident $(<$lifetime:lifetime>)?),* $(,)?) => {$(
        impl $family $(<$lifetime>)? {
            /// This sentence in `locale`, or its English text marked as a fallback.
            pub fn rich(self, locale: impl Borrow<Locale>) -> Rich {
                rich::render(*locale.borrow(), self)
            }
        }
    )*};
}

plain_lookup!(
    IssueMessage<'_>,
    IssueFilterMessage,
    IssueSidebarMessage,
    LabelMessage<'_>,
    MergeMessage<'_>,
    MilestoneMessage<'_>,
    PullMessage<'_>,
    ReviewMessage<'_>,
    ThreadMessage,
    TimelineNoteMessage,
);
rich_lookup!(CollabSentence<'_>, TimelineEventMessage);
