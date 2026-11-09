//! Message families that a locale may leave untranslated.
//!
//! English is the complete source catalog: every family writes its English text, and a
//! translated catalog adds entries only for the messages it has translated. A lookup
//! in a locale without an entry returns the English text and says so, so a template
//! can mark exactly that text `lang="en"` while translated text carries no wrapper.
//! The diagnostic catalogs (`en-XA`, `ar-XB`) derive from English and never fall back.
use std::fmt::{self, Write};

use super::{Catalog, LocalizedText};
use crate::locale::Locale;
use crate::plural::Plural;

/// Writes one message: catalog wording through [`MessageWriter::literal`], untrusted
/// values through [`MessageWriter::argument`] so diagnostic catalogs can isolate them.
#[derive(Debug)]
pub struct MessageWriter {
    catalog: Catalog,
    output: String,
}

impl MessageWriter {
    /// Catalog wording; expanded by the expansion diagnostic.
    pub fn literal(&mut self, text: &str) {
        self.catalog.literal(&mut self.output, text);
    }

    /// An interpolated value (a name, an environment, a list), copied as plain text.
    pub fn argument(&mut self, value: &str) {
        self.catalog.argument(&mut self.output, value);
    }

    /// A count or other quantity, with the digit grouping of the language being written
    /// (`1,234`, `1.234`, `1 234`; see [`crate::number`]).
    pub fn number(&mut self, value: u64) {
        crate::number::write_integer(&mut self.output, self.catalog.locale, value);
    }

    /// An integer read as a label rather than a quantity (a year, a day of the month):
    /// plain ASCII digits, never grouped.
    pub fn digits(&mut self, value: i64) {
        let _ = write!(self.output, "{value}");
    }

    /// The CLDR plural category of `count` in the language being written, for choosing
    /// a noun form: `match text.plural(count) { Plural::One => .., _ => .. }`.
    pub const fn plural(&self, count: u64) -> Plural {
        Plural::of(self.catalog.locale, count)
    }

    fn finish(self) -> LocalizedText {
        self.catalog.finish(self.output)
    }
}

/// A typed message family whose only required catalog is English.
///
/// Adding a variant needs nothing but its English arm: [`MessageFamily::translate`]
/// defaults to "not translated" for every other locale, which then falls back.
pub trait MessageFamily: Copy {
    /// Stable catalog identifier, independent of wording and arguments.
    fn id(self) -> &'static str;

    /// The complete English source text.
    fn english(self, text: &mut MessageWriter);

    /// The English text when it is one fixed literal, which English pages then borrow
    /// instead of copying. `english` must write the same literal.
    fn english_literal(self) -> Option<&'static str> {
        None
    }

    /// Writes this message in `locale` and returns `true`, or returns `false` when the
    /// locale has no translation yet. Only real translated locales reach this method.
    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        let _ = (locale, text);
        false
    }
}

/// Text for presentation, and whether it is the English fallback of an untranslated
/// message. Plain Unicode text, escaped by templates like [`LocalizedText`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Localized {
    text: LocalizedText,
    fallback: bool,
    locale: Locale,
}

impl Localized {
    /// Text that exists only in English (a core relative timestamp, say), shown in a page
    /// rendered for `locale`: a fallback wherever that locale is a real translation.
    pub fn english_only(text: String, locale: Locale) -> Self {
        Self { text: LocalizedText(text.into()), fallback: !derives_from_english(locale), locale }
    }

    /// Whether the text borrows a compiled literal rather than owning a copy.
    #[cfg(test)]
    pub(crate) const fn borrows_static(&self) -> bool {
        matches!(self.text.0, std::borrow::Cow::Borrowed(_))
    }

    /// The text alone, without its fallback mark.
    pub(crate) fn into_text(self) -> LocalizedText {
        self.text
    }

    /// The text to display.
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }

    /// `true` when the page's locale has no translation and this is the English text.
    pub const fn fallback(&self) -> bool {
        self.fallback
    }

    /// The `lang` a template declares around this text: `en` for a fallback; otherwise
    /// `None` in English, Spanish and the diagnostics, and the locale's own tag in the other
    /// translations, where a page may still hold English feature content around the text.
    pub const fn marked_lang(&self) -> Option<&'static str> {
        if self.fallback {
            Some(Locale::English.tag())
        } else if marks_nothing(self.locale) {
            None
        } else {
            Some(self.locale.tag())
        }
    }
}

impl fmt::Display for Localized {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.text.as_str())
    }
}

/// Locales whose translated text carries no `lang` of its own: English, its diagnostics,
/// and Spanish, the first translation, whose pages have always declared Spanish.
const fn marks_nothing(locale: Locale) -> bool {
    matches!(locale, Locale::English | Locale::Spanish | Locale::PseudoExpanded | Locale::PseudoRtl)
}

/// English itself and the diagnostic catalogs built from it.
const fn derives_from_english(locale: Locale) -> bool {
    matches!(locale, Locale::English | Locale::PseudoExpanded | Locale::PseudoRtl)
}

impl Catalog {
    /// Formats a family message in this catalog's locale, falling back to English.
    pub fn localized<M: MessageFamily>(self, message: M) -> Localized {
        localize(self, message)
    }
}

/// Kept apart from the `Catalog` type, whose cumulative definition has a size budget.
fn localize<M: MessageFamily>(catalog: Catalog, message: M) -> Localized {
    if !derives_from_english(catalog.locale) {
        let mut translated = MessageWriter { catalog, output: String::new() };
        if message.translate(catalog.locale, &mut translated) {
            return Localized { text: translated.finish(), fallback: false, locale: catalog.locale };
        }
        let mut english = MessageWriter { catalog: Catalog::new(Locale::English), output: String::new() };
        message.english(&mut english);
        return Localized { text: english.finish(), fallback: true, locale: catalog.locale };
    }
    if catalog.locale == Locale::English
        && let Some(literal) = message.english_literal()
    {
        return Localized { text: LocalizedText(literal.into()), fallback: false, locale: catalog.locale };
    }
    let mut english = MessageWriter { catalog, output: String::new() };
    message.english(&mut english);
    Localized { text: english.finish(), fallback: false, locale: catalog.locale }
}
