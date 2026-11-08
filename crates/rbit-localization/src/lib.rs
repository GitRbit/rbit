//! Locale negotiation and compiled, typed presentation messages.
//!
//! This crate owns no I/O, account state, cookies, HTML or runtime catalog parser.
//! The HTTP adapter validates preference provenance and persists choices; templates
//! escape [`LocalizedText`] just like any other text. See `localization-contracts.md`
//! for supported grammar and the future translated-catalog adapter boundary.

mod catalog;
mod locale;
mod negotiation;
mod pseudo;

pub use catalog::collab::{
    ChecksMessage, CloseReason, CollabSentence, IssueFilterMenu, IssueFilterMessage, IssueMessage, IssueSidebarMessage, LabelMessage,
    MergeMessage, MilestoneMessage, PullMessage, ReviewMessage, Rich, RichFamily, RichPart, RichWriter, ThreadMessage,
    TimelineEventMessage, TimelineNoteMessage,
};
pub use catalog::settings;
pub use catalog::shell;
pub use catalog::{
    ActionsDisplay, ActionsEnvironmentMessage, ActionsJobMessage, ActionsMessage, ActionsOverviewMessage, ActionsRunnersMessage,
    ActionsSecretsMessage, ActionsSettingsMessage, Catalog, ChromeDisplay, ChromeMessage, JobDisplay, LanguageMessage, Localized,
    LocalizedDisplay, LocalizedText, Message, MessageFamily, MessageWriter, OverviewControlDisplay, OverviewControlMessage,
    RepositoryVisibility, RunPageMessage, SettingsNavMessage, ThemeMessage, ThemePreviewMessage,
};
pub use catalog::{
    ContributionLabel, ContributionMessage, ReleaseLabel, ReleaseMessage, RepoBrowseLabel, RepoBrowseMessage, RepoCreateLabel,
    RepoCreateMessage, RepoEditLabel, RepoEditMessage, RepoHistoryLabel, RepoHistoryMessage, RepoNavLabel, RepoNavMessage,
    RepoSettingsLabel, RepoSettingsMessage,
};
pub use locale::{Direction, Locale};
pub use negotiation::{LocalePreferences, LocaleResolver, PreferenceSource, ResolvedLocale, UnsupportedDefault};
