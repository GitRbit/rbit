//! Text display rules shared by fixed-height code and log rows.

/// The character a row of code shows for `ch`: itself, or a visible stand-in when `ch`
/// would break the line.
///
/// Code is shown one line per fixed-height row (`white-space: pre`, see
/// [`crate::blocks`]), so a character that ends a line inside a row would draw a
/// second line over the next row. Those are Unicode's mandatory breaks (UAX #14 classes
/// BK, CR, LF, NL): a carriage return reaches the DOM as a line feed (HTML's input stream
/// preprocessing turns every CR into LF), and engines differ on which of the others they
/// honour under `pre`, so all of them become Control Pictures. Copied text then has the
/// symbol instead of the character, which loses nothing that worked before: a copied CR
/// was already a line feed.
#[must_use]
#[inline]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_show_line_breaks_as_symbols() {
        assert_eq!(row_text("foo\rbar"), "foo\u{240D}bar");
        assert_eq!(row_text("a\u{0B}b\u{0C}c\u{85}d\u{2028}e\u{2029}f\ng"), "a␋b␌c␤d␤e␤f␊g");
        assert!(matches!(row_text("plain\ttext"), std::borrow::Cow::Borrowed("plain\ttext")));
    }
}
