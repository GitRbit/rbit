//! Text of the Actions settings pages in the page's locale. Messages there may be
//! untranslated: each is [`Localized`], and templates mark it English through
//! `ui::localized` (or `am::rich`) only where it fell back.
//!
//! A template builds one handle, `{% let t = crate::views::actions::ui_text::UiText::new(*page.locale) %}`,
//! and renders literal messages with `t.get(...)` and typed row fields with `t.get(*row.field)`.
use rbit_localization::{ActionsMessage, Catalog, ChromeMessage, Locale, Localized, MessageFamily};

/// Placeholders for the styled arguments of a [`UiText::rich`] message: private-use
/// characters, so no name, time or translation can be mistaken for one.
pub const SLOTS: [&str; 4] = ["\u{E000}", "\u{E001}", "\u{E002}", "\u{E003}"];

/// The page's catalog, copied into every template and macro that shows Actions text.
#[derive(Clone, Copy, Debug)]
pub struct UiText {
    catalog: Catalog,
}

impl UiText {
    pub const fn new(locale: Locale) -> Self {
        Self { catalog: Catalog::new(locale) }
    }

    /// The locale this text is written in.
    pub const fn locale(self) -> Locale {
        self.catalog.locale()
    }

    /// One message, falling back to English where the locale has not translated it.
    pub fn get<M: MessageFamily>(self, message: M) -> Localized {
        self.catalog.localized(message)
    }

    /// Text that only exists in English, such as a service's error message.
    pub fn english(self, text: &str) -> Localized {
        Localized::english_only(text.to_owned(), self.catalog.locale())
    }

    /// "Actions", as the run pages name it.
    pub fn actions(self) -> Localized {
        self.catalog.localized(ActionsMessage::Actions)
    }

    /// "Settings", as the shell names it.
    pub fn settings(self) -> Localized {
        self.catalog.localized(ChromeMessage::Settings)
    }

    /// `message` formatted with [`SLOTS`] in place of its styled arguments, then cut into
    /// pieces: `SLOTS[i]` becomes `marks[i]`, wherever the translation puts it.
    pub fn rich<M: MessageFamily>(self, message: M, marks: &[Mark<'_>]) -> Rich {
        let localized = self.catalog.localized(message);
        Rich { fallback: localized.fallback(), pieces: split(localized.as_str(), marks) }
    }

    /// Several messages as one line, separated by " · ", English if any part is.
    pub fn joined(self, parts: impl IntoIterator<Item = Localized>) -> Rich {
        let mut fallback = false;
        let mut text = String::new();
        for part in parts {
            if !text.is_empty() {
                text.push_str(" · ");
            }
            fallback |= part.fallback();
            text.push_str(part.as_str());
        }
        Rich { fallback, pieces: vec![Piece::plain(&text)] }
    }
}

/// How one styled argument of a [`UiText::rich`] message is shown.
#[derive(Clone, Copy, Debug)]
pub struct Mark<'a> {
    value: &'a str,
    class: &'static str,
    ltr: bool,
    datetime: Option<&'a str>,
}

impl<'a> Mark<'a> {
    /// A machine value (code, a path, a key) in `class`, always left to right.
    pub const fn code(value: &'a str, class: &'static str) -> Self {
        Self { value, class, ltr: true, datetime: None }
    }

    /// A name in `class`, following the page direction.
    pub const fn name(value: &'a str, class: &'static str) -> Self {
        Self { value, class, ltr: false, datetime: None }
    }

    /// A relative time, in a `<time datetime>` element.
    pub const fn time(value: &'a str, iso: &'a str) -> Self {
        Self { value, class: "", ltr: false, datetime: Some(iso) }
    }
}

/// Formatted message text cut into plain and marked pieces; rendered by `am::rich`.
#[derive(Debug)]
pub struct Rich {
    pub fallback: bool,
    pub pieces: Vec<Piece>,
}

/// One piece of a [`Rich`] message: plain text when `class` is empty and it has no time.
#[derive(Debug, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub class: &'static str,
    pub ltr: bool,
    pub datetime: Option<String>,
}

impl Piece {
    fn plain(text: &str) -> Self {
        Self { text: text.to_owned(), class: "", ltr: false, datetime: None }
    }
}

fn split(text: &str, marks: &[Mark<'_>]) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some((at, index)) = rest.char_indices().find_map(|(at, c)| slot(c).map(|index| (at, index))) {
        if at > 0 {
            pieces.push(Piece::plain(&rest[..at]));
        }
        if let Some(mark) = marks.get(index) {
            pieces.push(Piece {
                text: mark.value.to_owned(),
                class: mark.class,
                ltr: mark.ltr,
                datetime: mark.datetime.map(str::to_owned),
            });
        }
        rest = &rest[at + SLOTS[index].len()..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::plain(rest));
    }
    pieces
}

fn slot(c: char) -> Option<usize> {
    SLOTS.iter().position(|slot| slot.starts_with(c))
}

#[cfg(test)]
mod tests {
    use rbit_localization::{ActionsEnvironmentMessage, ActionsSecretsMessage};

    use super::*;

    #[test]
    fn slots_become_their_marks_and_names_never_match_the_wording() {
        let text = UiText::new(Locale::English);
        let marks = [
            Mark::code("release/*", "font-mono"),
            Mark::code("refs/tags/", "font-mono"),
            Mark::code("refs/tags/v*", "font-mono"),
            Mark::code("!", "font-mono"),
        ];
        let hint = ActionsEnvironmentMessage::PatternsHint { branch: SLOTS[0], tags: SLOTS[1], tag: SLOTS[2], exclude: SLOTS[3] };
        let rich = text.rich(hint, &marks);
        let marked: Vec<&str> = rich.pieces.iter().filter(|piece| piece.ltr).map(|piece| piece.text.as_str()).collect();
        assert_eq!(marked, ["release/*", "refs/tags/", "refs/tags/v*", "!"]);
        let joined: String = rich.pieces.iter().map(|piece| piece.text.as_str()).collect();
        let plain = ActionsEnvironmentMessage::PatternsHint { branch: "release/*", tags: "refs/tags/", tag: "refs/tags/v*", exclude: "!" };
        assert_eq!(joined, text.get(plain).as_str());
        assert!(!rich.fallback);
        let named = text.rich(ActionsSecretsMessage::NewValueOf { name: SLOTS[0] }, &[Mark::code("N", "font-mono")]);
        assert_eq!(named.pieces[0], Piece::plain("New value of "), "the N of New stays wording");
    }

    #[test]
    fn spanish_translates_and_keeps_times() {
        let rich = UiText::new(Locale::Spanish)
            .rich(ActionsSecretsMessage::Updated { when: SLOTS[0] }, &[Mark::time("2 days ago", "2026-11-06T00:00:00Z")]);
        assert!(!rich.fallback);
        assert_eq!(rich.pieces[0], Piece::plain("Actualizado "));
        assert_eq!(rich.pieces[1].text, "2 days ago");
        assert_eq!(rich.pieces[1].datetime.as_deref(), Some("2026-11-06T00:00:00Z"));
    }

    #[test]
    fn rtl_isolates_stay_around_the_marked_piece() {
        let rich =
            UiText::new(Locale::PseudoRtl).rich(ActionsSecretsMessage::ValueOf { name: SLOTS[0] }, &[Mark::code("REGION", "font-mono")]);
        let text: String = rich.pieces.iter().map(|piece| piece.text.as_str()).collect();
        assert!(text.contains("\u{2068}REGION\u{2069}"), "{text}");
    }
}
