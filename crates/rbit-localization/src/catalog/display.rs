//! Plain lazy messages, escaped by the caller just like `LocalizedText`. English dynamic
//! labels write straight to the destination; other locales write their localized text.
use super::{Catalog, Message};
use crate::Locale;
use std::fmt;

pub(super) const SUBTITLE_PREFIX: &str = "Public repositories on ";
pub(super) const SUBTITLE_SUFFIX: &str = ".";
pub(super) const PAGE_PREFIX: &str = "Page ";
pub(super) const PAGE_OF_SEPARATOR: &str = " of ";
pub(super) const STARS_SUFFIX: &str = " stars";

/// A borrowed typed message, never trusted HTML. Ordinary Display interpolation
/// preserves the template's escaping boundary; there is deliberately no `as_str`,
/// HTML-safety implementation, parser, owned argument or pre-rendered markup.
#[derive(Clone, Copy, Debug)]
pub struct LocalizedDisplay<'a> {
    catalog: Catalog,
    message: Message<'a>,
}
impl Catalog {
    /// Defers formatting until consumed. English Explore dynamic labels
    /// write straight to the destination; other catalogs retain `text()` semantics.
    pub const fn display(self, message: Message<'_>) -> LocalizedDisplay<'_> {
        LocalizedDisplay { catalog: self, message }
    }
}
impl fmt::Display for LocalizedDisplay<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.catalog.locale() == Locale::English
            && let Message::ExploreSubtitle { site } = self.message
        {
            return write!(output, "{SUBTITLE_PREFIX}{site}{SUBTITLE_SUFFIX}");
        }
        fmt::Display::fmt(&self.catalog.localized(self.message), output)
    }
}

impl LocalizedDisplay<'_> {
    /// The `lang` a template declares around this text, as [`super::Localized::marked_lang`].
    pub fn marked_lang(&self) -> Option<&'static str> {
        match self.catalog.locale() {
            Locale::English => None,
            _ => self.catalog.localized(self.message).marked_lang(),
        }
    }
}
