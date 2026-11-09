//! Compiled messages for the first repository-listing slice; no runtime parsing.

use std::{borrow::Cow, fmt};

use crate::{Locale, pseudo};
mod overview;
mod overview_controls;
pub use overview::ActionsOverviewMessage;
pub use overview_controls::{OverviewControlDisplay, OverviewControlMessage};
mod actions;
pub use actions::{ActionsDisplay, ActionsMessage};
mod job;
pub use job::{ActionsJobMessage, JobDisplay};
mod display;
mod language;
mod theme;
pub use language::LanguageMessage;
pub use theme::{ThemeMessage, ThemeSentence};
mod chrome;
pub use chrome::{ChromeDisplay, ChromeMessage, ChromeSentence};
mod fallback;
mod translations;
pub use fallback::{Localized, MessageFamily, MessageWriter};
mod run_page;
pub use run_page::RunPageMessage;
pub mod settings;
mod settings_nav;
pub use settings_nav::SettingsNavMessage;
mod actions_ui;
pub(crate) mod collab;
pub mod shell;
mod theme_preview;
pub mod time;
pub use actions_ui::{
    environments::ActionsEnvironmentMessage, runners::ActionsRunnersMessage, secrets::ActionsSecretsMessage,
    settings::ActionsSettingsMessage,
};
pub use display::LocalizedDisplay;
mod explore;
pub use explore::Message;
pub use theme_preview::ThemePreviewMessage;
mod repo;
pub use repo::{
    ContributionLabel, ContributionMessage, ReleaseLabel, ReleaseMessage, RepoBrowseLabel, RepoBrowseMessage, RepoBrowseSentence,
    RepoCreateLabel, RepoCreateMessage, RepoEditLabel, RepoEditMessage, RepoHistoryLabel, RepoHistoryMessage, RepoNavLabel, RepoNavMessage,
    RepoNavSentence, RepoSettingsLabel, RepoSettingsMessage,
};
mod wiki;
pub use wiki::WikiMessage;

/// Typed visibility selector. Adapters map domain values without localized strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryVisibility {
    Public,
    Private,
    Internal,
}

/// Plain Unicode text, never an HTML fragment. Use ordinary Askama interpolation;
/// `safe` or unescaped `innerHTML` would violate this type's presentation contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedText(Cow<'static, str>);

impl LocalizedText {
    /// Borrows plain text for templates, text mail, accessible labels or CLI output.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LocalizedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A copyable catalog handle; all entries are compiled into the executable.
#[derive(Clone, Copy, Debug)]
pub struct Catalog {
    locale: Locale,
}

impl Catalog {
    /// Selects one complete catalog without allocation or parsing.
    pub const fn new(locale: Locale) -> Self {
        Self { locale }
    }

    /// Locale used for text, document language/direction and cache dimensions.
    pub const fn locale(self) -> Locale {
        self.locale
    }

    fn finish(self, mut output: String) -> LocalizedText {
        if matches!(self.locale, Locale::PseudoExpanded | Locale::PseudoRtl) {
            output.insert(0, '[');
            output.push(']');
        }
        LocalizedText(Cow::Owned(output))
    }

    fn literal(self, output: &mut String, text: &str) {
        match self.locale {
            Locale::PseudoExpanded => pseudo::expand(output, text),
            _ => output.push_str(text),
        }
    }

    fn argument(self, output: &mut String, value: &str) {
        if self.locale == Locale::PseudoRtl {
            output.push('\u{2068}');
        }
        output.push_str(value);
        if self.locale == Locale::PseudoRtl {
            output.push('\u{2069}');
        }
    }
}

#[cfg(test)]
mod tests;
