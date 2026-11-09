//! Turning repository content into safe HTML.

mod document_cache;
pub mod document_jobs;
mod highlight;
mod highlight_cache;
#[cfg(test)]
mod markdown_tests;
pub mod references;

pub use document_cache::{DocumentCache, DocumentCacheStats};
pub use highlight::Highlighter;
pub use highlight_cache::{CacheStats, HIGHLIGHTER_VERSION, HighlightCache, HighlightedFile};
use rbit_presentation::markdown::{self, LinkResolver};
pub use rbit_presentation::markdown::{Rewritten, escape_html};

use crate::urls::DocumentBase;

/// Renders untrusted Markdown with [`rbit_presentation::markdown::render`]; safe to insert
/// into a page verbatim.
///
/// With a `base`, relative links and images resolve against the document's location in
/// its repository (see [`DocumentBase::resolve`]); without one they are left as written.
/// Documents with a base (READMEs, Markdown files, release notes) are kept in
/// [`DocumentCache::shared`], so viewing one again copies the stored HTML instead of
/// rendering it.
pub fn render_markdown(source: &str, base: Option<&DocumentBase>) -> String {
    match base {
        Some(base) => DocumentCache::shared().get_or_render(source, base, || markdown::render(source, Some(base as &dyn LinkResolver))),
        None => markdown::render(source, None),
    }
}

/// [`render_markdown`] without the cache, with every run of plain text offered to
/// `rewrite` (see [`rbit_presentation::markdown::render_rewriting`]).
pub fn render_markdown_rewriting(source: &str, base: Option<&DocumentBase>, rewrite: &dyn Fn(&str) -> Option<Rewritten>) -> String {
    markdown::render_rewriting(source, base.map(|base| base as &dyn LinkResolver), rewrite)
}

/// The character a row of code shows for `ch`: itself, or a visible stand-in when `ch`
/// would break the line.
///
/// Code is shown one line per fixed-height row (`white-space: pre`, see
/// [`crate::views::blocks`]), so a character that ends a line inside a row would draw a
/// second line over the next row. Those are Unicode's mandatory breaks (UAX #14 classes
/// BK, CR, LF, NL): a carriage return reaches the DOM as a line feed (HTML's input stream
/// preprocessing turns every CR into LF), and engines differ on which of the others they
/// honour under `pre`, so all of them become Control Pictures. Copied text then has the
/// symbol instead of the character, which loses nothing that worked before: a copied CR
/// was already a line feed.
#[must_use]
pub fn row_char(ch: char) -> char {
    match ch {
        '\n' => '\u{240A}',                               // ␊
        '\u{0B}' => '\u{240B}',                           // ␋
        '\u{0C}' => '\u{240C}',                           // ␌
        '\r' => '\u{240D}',                               // ␍
        '\u{85}' | '\u{2028}' | '\u{2029}' => '\u{2424}', // ␤
        other => other,
    }
}

/// `text` with every character replaced by its [`row_char`], borrowed when nothing changes.
#[must_use]
pub fn row_text(text: &str) -> std::borrow::Cow<'_, str> {
    if text.chars().all(|ch| row_char(ch) == ch) { text.into() } else { text.chars().map(row_char).collect::<String>().into() }
}

/// Whether a file name denotes Markdown that should be rendered rather than shown as source.
pub fn is_markdown(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    [".md", ".markdown", ".mdown", ".mkd"].iter().any(|ext| lower.ends_with(ext))
}

/// Human-readable byte size (`1.4 KB`, `1,4 KB` in German).
pub fn format_size(bytes: u64, locale: rbit_localization::Locale) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", rbit_localization::number::integer(locale, bytes))
    } else {
        format!("{} {}", crate::views::when::decimal(locale, value), UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_show_line_breaks_as_symbols() {
        assert_eq!(row_text("foo\rbar"), "foo\u{240D}bar");
        assert_eq!(row_text("a\u{0B}b\u{0C}c\u{85}d\u{2028}e\u{2029}f\ng"), "a␋b␌c␤d␤e␤f␊g");
        assert!(matches!(row_text("plain\ttext"), std::borrow::Cow::Borrowed("plain\ttext")));
    }

    #[test]
    fn sizes() {
        let english = rbit_localization::Locale::English;
        assert_eq!(format_size(512, english), "512 B");
        assert_eq!(format_size(1536, english), "1.5 KB");
        assert_eq!(format_size(1536, rbit_localization::Locale::French), "1,5 KB");
    }
}
