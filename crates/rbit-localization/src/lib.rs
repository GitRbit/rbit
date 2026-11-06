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

pub use catalog::{
    ActionsDisplay, ActionsJobMessage, ActionsMessage, ActionsOverviewMessage, Catalog, JobDisplay, LocalizedDisplay, LocalizedText,
    Message, RepositoryVisibility,
};
pub use locale::{Direction, Locale};
pub use negotiation::{LocalePreferences, LocaleResolver, PreferenceSource, ResolvedLocale, UnsupportedDefault};
