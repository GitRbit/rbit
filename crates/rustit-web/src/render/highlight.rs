//! Syntax highlighting with syntect, emitting CSS classes (`hl-keyword`, `hl-string`, …)
//! instead of inline colours. Colours therefore come from the design-system tokens and
//! follow light/dark mode automatically, and the strict CSP (no inline styles) holds.

use std::{fmt::Write, sync::OnceLock};

use syntect::{
    html::{ClassStyle, line_tokens_to_classed_spans},
    parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };

/// Files above these limits are shown as plain text: highlighting cost grows with size
/// and pathological inputs (minified bundles) gain nothing from it.
const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;
const MAX_LINE_LEN: usize = 5_000;

/// Loads syntax definitions lazily on first use (≈ tens of ms), then shares them.
#[derive(Debug, Default)]
pub struct Highlighter {
    syntaxes: OnceLock<SyntaxSet>,
}

impl Highlighter {
    pub fn new() -> Self {
        Self::default()
    }

    fn syntaxes(&self) -> &SyntaxSet {
        self.syntaxes.get_or_init(SyntaxSet::load_defaults_newlines)
    }

    /// Name of the detected language, e.g. `Rust`, or `None` for plain text.
    pub fn language_name(&self, file_name: &str, text: &str) -> Option<String> {
        self.detect(file_name, text).map(|s| s.name.clone())
    }

    fn detect(&self, file_name: &str, text: &str) -> Option<&SyntaxReference> {
        let set = self.syntaxes();
        let extension = file_name.rsplit_once('.').map_or(file_name, |(_, ext)| ext);
        set.find_syntax_by_extension(extension)
            .or_else(|| set.find_syntax_by_extension(file_name))
            .or_else(|| text.lines().next().and_then(|first| set.find_syntax_by_first_line(first)))
            .filter(|syntax| syntax.name != "Plain Text")
    }

    /// Highlights `text` into one self-contained HTML fragment per line: spans that cross
    /// line boundaries are closed and re-opened, so each line can live in its own table row.
    pub fn highlight_lines(&self, file_name: &str, text: &str) -> Vec<String> {
        let syntax = self.detect(file_name, text).filter(|_| text.len() <= MAX_HIGHLIGHT_BYTES);
        let Some(syntax) = syntax else {
            return plain_lines(text);
        };

        let set = self.syntaxes();
        let mut parser = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut lines = Vec::new();

        for line in LinesWithEndings::from(text) {
            if line.len() > MAX_LINE_LEN {
                return plain_lines(text);
            }
            let Ok(ops) = parser.parse_line(line, set) else {
                return plain_lines(text);
            };

            let mut html = String::with_capacity(line.len() * 2);
            for scope in stack.as_slice() {
                open_span(&mut html, &scope.build_string());
            }
            let Ok((spans, _)) = line_tokens_to_classed_spans(line, &ops, CLASS_STYLE, &mut stack) else {
                return plain_lines(text);
            };
            // The line terminator may sit inside a span (e.g. a line comment); each line is
            // rendered in its own row, so terminators are dropped wherever they appear.
            html.extend(spans.chars().filter(|c| !matches!(c, '\n' | '\r')));
            for _ in 0..stack.len() {
                html.push_str("</span>");
            }
            lines.push(html);
        }
        lines
    }
}

/// Mirrors syntect's `SpacedPrefixed` class format: `source.rust` → `hl-source hl-rust`.
fn open_span(out: &mut String, scope: &str) {
    out.push_str("<span class=\"");
    for (i, atom) in scope.split('.').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "hl-{atom}");
    }
    out.push_str("\">");
}

fn plain_lines(text: &str) -> Vec<String> {
    text.lines().map(escape_html).collect()
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_with_balanced_spans_per_line() {
        let highlighter = Highlighter::new();
        let lines = highlighter.highlight_lines("main.rs", "/* multi\nline */\nfn main() {}\n");
        assert_eq!(lines.len(), 3);
        for line in &lines {
            assert_eq!(line.matches("<span").count(), line.matches("</span>").count(), "unbalanced: {line}");
        }
        assert!(lines[2].contains("hl-keyword") || lines[2].contains("hl-storage"));
    }

    #[test]
    fn unknown_files_are_escaped_plain_text() {
        let lines = Highlighter::new().highlight_lines("notes.unknownext", "<b>hi</b>");
        assert_eq!(lines, vec!["&lt;b&gt;hi&lt;/b&gt;"]);
    }
}
