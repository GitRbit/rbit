//! Sentences that embed markup: a link, a time, a branch name, a count in its own element.
//!
//! A rich message is written like any other, except that each embedded piece is a numbered
//! slot. The template renders the slots (it knows their markup) and the text between them,
//! so a translation may move or reorder the pieces without splitting a sentence into
//! fragments. Fallback follows [`super::super::fallback`]: a locale without a translation
//! gets the English sentence, marked as such.
use std::fmt::Write;

use crate::{catalog::Catalog, locale::Locale, plural::Plural};

/// Writes one rich message: wording, plain arguments, numbers and slots.
#[derive(Debug)]
pub struct RichWriter {
    catalog: Catalog,
    parts: Vec<RichPart>,
}

impl RichWriter {
    const fn new(catalog: Catalog) -> Self {
        Self { catalog, parts: Vec::new() }
    }

    /// Catalog wording; expanded by the expansion diagnostic.
    pub fn literal(&mut self, text: &str) {
        let catalog = self.catalog;
        catalog.literal(self.text(), text);
    }

    /// A plain interpolated value (a name, a number with its sign), copied as text.
    pub fn argument(&mut self, value: &str) {
        let catalog = self.catalog;
        catalog.argument(self.text(), value);
    }

    /// An integer, written with ASCII digits.
    pub fn number(&mut self, value: u64) {
        let _ = write!(self.text(), "{value}");
    }

    /// The CLDR plural category of `count` in the language being written.
    pub const fn plural(&self, count: u64) -> Plural {
        Plural::of(self.catalog.locale(), count)
    }

    /// The embedded piece numbered `index`; each message documents what its slots hold.
    pub fn slot(&mut self, index: usize) {
        let isolate = self.catalog.locale() == Locale::PseudoRtl;
        if isolate {
            self.text().push('\u{2068}');
        }
        self.parts.push(RichPart { text: String::new(), slot: Some(index) });
        if isolate {
            self.text().push('\u{2069}');
        }
    }

    /// The text part being written, started after a slot.
    fn text(&mut self) -> &mut String {
        if self.parts.last().is_none_or(|part| part.slot.is_some()) {
            self.parts.push(RichPart::default());
        }
        let last = self.parts.len() - 1;
        &mut self.parts[last].text
    }

    fn finish(mut self, fallback: bool) -> Rich {
        if matches!(self.catalog.locale(), Locale::PseudoExpanded | Locale::PseudoRtl) {
            self.parts.insert(0, RichPart { text: "[".to_owned(), slot: None });
            self.text().push(']');
        }
        Rich { parts: self.parts, fallback }
    }
}

/// One piece of a rich message: text, or a slot the template fills in.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RichPart {
    text: String,
    slot: Option<usize>,
}

impl RichPart {
    /// The plain text of a text part (empty for a slot).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The slot number, for a slot.
    pub const fn slot(&self) -> Option<usize> {
        self.slot
    }

    /// Whether this is slot `index`, for templates.
    pub fn is_slot(&self, index: usize) -> bool {
        self.slot == Some(index)
    }
}

/// A formatted rich message, and whether it is the English fallback of an untranslated
/// message (templates then mark it `lang="en"`). Its text is plain and escaped by templates.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Rich {
    parts: Vec<RichPart>,
    fallback: bool,
}

impl Rich {
    /// Text and slots, in reading order.
    pub fn parts(&self) -> &[RichPart] {
        &self.parts
    }

    /// `true` when the page's locale has no translation and this is the English text.
    pub const fn fallback(&self) -> bool {
        self.fallback
    }

    /// The sentence with slot `n` replaced by `slots[n]` (nothing when missing), for tests,
    /// `title` attributes and other plain-text uses.
    pub fn plain(&self, slots: &[&str]) -> String {
        self.parts.iter().map(|part| part.slot.map_or(part.text.as_str(), |index| slots.get(index).copied().unwrap_or_default())).collect()
    }
}

/// A typed family of rich messages whose only required catalog is English, like
/// [`super::super::fallback::MessageFamily`].
pub trait RichFamily: Copy {
    /// Stable catalog identifier, independent of wording and arguments.
    fn id(self) -> &'static str;

    /// The complete English source text.
    fn english(self, text: &mut RichWriter);

    /// Writes this message in `locale` and returns `true`, or returns `false` when the
    /// locale has no translation yet.
    fn translate(self, locale: Locale, text: &mut RichWriter) -> bool {
        let _ = (locale, text);
        false
    }
}

/// Formats `message` in `locale`, falling back to English.
pub(crate) fn render<M: RichFamily>(locale: Locale, message: M) -> Rich {
    if !matches!(locale, Locale::English | Locale::PseudoExpanded | Locale::PseudoRtl) {
        let mut translated = RichWriter::new(Catalog::new(locale));
        if message.translate(locale, &mut translated) {
            return translated.finish(false);
        }
        let mut english = RichWriter::new(Catalog::new(Locale::English));
        message.english(&mut english);
        return english.finish(true);
    }
    let mut english = RichWriter::new(Catalog::new(locale));
    message.english(&mut english);
    english.finish(false)
}
