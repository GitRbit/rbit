//! Complete settings messages for the production language preference.
use super::fallback::{MessageFamily, MessageWriter};
use crate::Locale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LanguageMessage {
    Title,
    Label,
    BrowserDefault,
    BrowserHelp,
    Coverage,
    English,
    Spanish,
    Save,
    Saved,
    Invalid,
}
impl LanguageMessage {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Title => "settings.language.title",
            Self::Label => "settings.language.label",
            Self::BrowserDefault => "settings.language.browser-default",
            Self::BrowserHelp => "settings.language.browser-help",
            Self::Coverage => "settings.language.coverage",
            Self::English => "settings.language.english",
            Self::Spanish => "settings.language.spanish",
            Self::Save => "settings.language.save",
            Self::Saved => "settings.language.saved",
            Self::Invalid => "settings.language.invalid",
        }
    }
}
impl LanguageMessage {
    /// The English text.
    const fn english_label(self) -> &'static str {
        match self {
            Self::Title => "Display language",
            Self::Label => "Preferred language",
            Self::BrowserDefault => "Browser / default",
            Self::BrowserHelp => "Follow your browser's supported language, or use English when it is unavailable.",
            Self::Coverage => "Text that is not translated into your language yet is shown in English.",
            Self::English => "English",
            Self::Spanish => "Español",
            Self::Save => "Save language",
            Self::Saved => "Your language preference was saved.",
            Self::Invalid => "Choose one of the listed languages or the browser default.",
        }
    }
}

impl MessageFamily for LanguageMessage {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        text.literal(self.english_label());
    }

    fn english_literal(self) -> Option<&'static str> {
        Some(self.english_label())
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
