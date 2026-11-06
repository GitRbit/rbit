//! Plain lazy messages, escaped by the caller just like `LocalizedText`.
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
    /// Defers formatting until consumed. The three English Explore dynamic labels
    /// write straight to the destination; all other cases retain `text()` semantics.
    pub const fn display(self, message: Message<'_>) -> LocalizedDisplay<'_> {
        LocalizedDisplay { catalog: self, message }
    }
}
impl fmt::Display for LocalizedDisplay<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.catalog.locale() == Locale::English {
            match self.message {
                Message::Stars { count } => return write!(output, "{count}{STARS_SUFFIX}"),
                Message::PageOf { page, pages } => return write!(output, "{PAGE_PREFIX}{page}{PAGE_OF_SEPARATOR}{pages}"),
                Message::ExploreSubtitle { site } => return write!(output, "{SUBTITLE_PREFIX}{site}{SUBTITLE_SUFFIX}"),
                _ => {}
            }
        }
        if self.catalog.locale() == Locale::Spanish {
            use super::spanish::{PAGE_OF, PAGE_PREFIX, SUBTITLE_PREFIX};
            match self.message {
                Message::Stars { count } => {
                    return write!(output, "{count} {}", if count.unsigned_abs() == 1 { "estrella" } else { "estrellas" });
                }
                Message::PageOf { page, pages } => return write!(output, "{PAGE_PREFIX}{page}{PAGE_OF}{pages}"),
                Message::ExploreSubtitle { site } => return write!(output, "{SUBTITLE_PREFIX}{site}."),
                _ => {}
            }
        }
        fmt::Display::fmt(&self.catalog.text(self.message), output)
    }
}
