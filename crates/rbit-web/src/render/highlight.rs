//! Syntax highlighting with syntect, emitting short CSS classes (`k`, `s`, …) instead of
//! inline colours. Colours therefore come from the design-system tokens and follow
//! light/dark mode and tinted themes automatically, and the strict CSP (no inline styles)
//! holds.
//!
//! # Compact output
//!
//! syntect describes every token by a stack of scopes (`source.rust`, `meta.function.rust`,
//! `keyword.operator.rust`, …). Rendering that stack as nested spans, one class per scope
//! atom, makes a 3,000-line file about 2 MB of HTML in which most spans style nothing. The
//! stylesheet only gives a handful of *categories* a look, so the cascade is resolved here,
//! in Rust, once per token:
//!
//! * [`RULES`] mirrors the stylesheet the nested markup was written for, in source order.
//!   For each scope on the stack the colour comes from the matching rule with the highest
//!   specificity (number of atoms), the later one on a tie, exactly as CSS picks it.
//! * Each property is then inherited from the innermost scope that sets it (bold and
//!   italic are never reset, so any scope setting them applies).
//! * The result is one flat span per run of equally styled text, with one short class per
//!   styled category (see [`CLASSES`]); unstyled text is bare. Text is HTML-escaped
//!   with the same table syntect uses.
//!
//! app.css's **Syntax highlighting** section styles the short classes; the tests resolve
//! both the old nested markup (against the old rules) and the new markup (against the
//! stylesheet itself) per character and require identical results.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use syntect::{
    easy::ScopeRangeIterator,
    parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

use super::highlight_cache::{HighlightCache, HighlightedFile};
use crate::error::WebResult;

/// Files above these limits are shown as plain text: highlighting cost grows with size
/// and pathological inputs (minified bundles) gain nothing from it.
const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;
const MAX_LINE_LEN: usize = 5_000;

/// A styled category: one short class each, coloured by a `--syntax-*` token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Comment,
    String,
    Number,
    Keyword,
    Function,
    Type,
    Tag,
    Attribute,
    /// Markup headings: ink coloured and semibold.
    Heading,
    Invalid,
}

impl Color {
    /// The class app.css styles this category with (the first column of [`CLASSES`]),
    /// to check the table against.
    #[cfg(test)]
    const fn class(self) -> &'static str {
        match self {
            Self::Comment => "c",
            Self::String => "s",
            Self::Number => "n",
            Self::Keyword => "k",
            Self::Function => "f",
            Self::Type => "t",
            Self::Tag => "g",
            Self::Attribute => "a",
            Self::Heading => "h",
            Self::Invalid => "x",
        }
    }
}

/// What one stylesheet rule sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    Color(Color),
    Bold,
    Italic,
}

/// The highlighting rules, in stylesheet order: a scope matches a selector when it has
/// all of the selector's atoms (`.hl-constant.hl-numeric` matches `constant.numeric.hex`).
const RULES: &[(&[&str], Effect)] = &[
    (&["comment"], Effect::Color(Color::Comment)),
    (&["string"], Effect::Color(Color::String)),
    (&["markup", "raw"], Effect::Color(Color::String)),
    (&["constant", "numeric"], Effect::Color(Color::Number)),
    (&["constant", "language"], Effect::Color(Color::Number)),
    (&["constant", "character"], Effect::Color(Color::Number)),
    (&["keyword"], Effect::Color(Color::Keyword)),
    (&["storage"], Effect::Color(Color::Keyword)),
    (&["entity", "name", "function"], Effect::Color(Color::Function)),
    (&["support", "function"], Effect::Color(Color::Function)),
    (&["meta", "function-call"], Effect::Color(Color::Function)),
    (&["entity", "name", "type"], Effect::Color(Color::Type)),
    (&["entity", "name", "class"], Effect::Color(Color::Type)),
    (&["support", "type"], Effect::Color(Color::Type)),
    (&["support", "class"], Effect::Color(Color::Type)),
    (&["storage", "type"], Effect::Color(Color::Type)),
    (&["entity", "name", "tag"], Effect::Color(Color::Tag)),
    (&["entity", "other", "attribute-name"], Effect::Color(Color::Attribute)),
    (&["markup", "heading"], Effect::Color(Color::Heading)),
    (&["markup", "heading"], Effect::Bold),
    (&["markup", "bold"], Effect::Bold),
    (&["markup", "italic"], Effect::Italic),
    (&["invalid"], Effect::Color(Color::Invalid)),
];

/// The computed look of a run of text (or what one scope declares).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Style {
    color: Option<Color>,
    bold: bool,
    italic: bool,
}

impl Style {
    /// What a scope with these atoms declares, resolving colour conflicts like the cascade.
    fn declared_by(atoms: &[&str]) -> Self {
        let mut style = Self::default();
        let mut best: Option<(usize, usize)> = None;
        for (order, (selector, effect)) in RULES.iter().enumerate() {
            if !selector.iter().all(|atom| atoms.contains(atom)) {
                continue;
            }
            match *effect {
                Effect::Color(color) => {
                    let rank = (selector.len(), order);
                    if best.is_none_or(|best| rank > best) {
                        best = Some(rank);
                        style.color = Some(color);
                    }
                }
                Effect::Bold => style.bold = true,
                Effect::Italic => style.italic = true,
            }
        }
        style
    }

    /// The classes of a span with this style; `None` for unstyled text. A heading's
    /// class carries its weight, so `b` is left out under it. Looked up in [`CLASSES`],
    /// so opening a span allocates nothing.
    fn classes(self) -> Option<&'static str> {
        let color = self.color.map_or(0, |color| color as usize + 1);
        let class = CLASSES[color][usize::from(self.bold) * 2 + usize::from(self.italic)];
        (!class.is_empty()).then_some(class)
    }
}

/// The class attribute of every style: one row per colour (none first, then [`Color`] in
/// declaration order), one column per weight and slant (plain, italic, bold, bold
/// italic). Classes come in a fixed order: the colour's, `b`, `i`.
const CLASSES: [[&str; 4]; 11] = [
    ["", "i", "b", "b i"],
    ["c", "c i", "c b", "c b i"],
    ["s", "s i", "s b", "s b i"],
    ["n", "n i", "n b", "n b i"],
    ["k", "k i", "k b", "k b i"],
    ["f", "f i", "f b", "f b i"],
    ["t", "t i", "t b", "t b i"],
    ["g", "g i", "g b", "g b i"],
    ["a", "a i", "a b", "a b i"],
    // A heading's class carries its weight: no `b`.
    ["h", "h i", "h", "h i"],
    ["x", "x i", "x b", "x b i"],
];

/// Resolves scope stacks to styles, remembering what each scope declares.
#[derive(Debug, Default)]
struct Resolver {
    declared: HashMap<Scope, Style>,
}

impl Resolver {
    fn declared(&mut self, scope: Scope) -> Style {
        *self.declared.entry(scope).or_insert_with(|| {
            let name = scope.build_string();
            let atoms: Vec<&str> = name.split('.').collect();
            Style::declared_by(&atoms)
        })
    }

    /// Each property from the innermost scope that sets it.
    fn resolve(&mut self, stack: &[Scope]) -> Style {
        let mut style = Style::default();
        for &scope in stack.iter().rev() {
            let declared = self.declared(scope);
            if style.color.is_none() {
                style.color = declared.color;
            }
            style.bold |= declared.bold;
            style.italic |= declared.italic;
        }
        style
    }
}

/// Builds one line of flat spans, merging adjacent runs with the same style.
#[derive(Debug, Default)]
struct LineWriter {
    html: String,
    run: Option<Style>,
    /// Whether the current run opened a span (it is styled), for [`Self::close`].
    open: bool,
}

impl LineWriter {
    /// A writer for a line of `len` bytes of source; its markup is at least as long.
    fn with_capacity(len: usize) -> Self {
        Self { html: String::with_capacity(len), ..Self::default() }
    }

    /// Appends `text`, which holds no line terminator (the caller cuts it off).
    fn push(&mut self, style: Style, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.run != Some(style) {
            self.close();
            if let Some(classes) = style.classes() {
                self.html.push_str("<span class=\"");
                self.html.push_str(classes);
                self.html.push_str("\">");
                self.open = true;
            }
            self.run = Some(style);
        }
        for ch in text.chars() {
            push_escaped(&mut self.html, super::row_char(ch));
        }
    }

    fn close(&mut self) {
        if self.open {
            self.html.push_str("</span>");
            self.open = false;
        }
        self.run = None;
    }

    fn finish(mut self) -> String {
        self.close();
        self.html
    }
}

/// Loads syntax definitions lazily on first use (≈ tens of ms), then shares them, and
/// keeps recently highlighted files ([`Self::highlight_lines_cached`]).
#[derive(Debug, Default, Clone)]
pub struct Highlighter {
    syntaxes: Arc<OnceLock<SyntaxSet>>,
    cache: HighlightCache,
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

    /// [`Self::highlight_lines`] through the cache. `content_id` must name `text` exactly:
    /// a Git blob id, or a Git LFS object id.
    /// The source is owned by a cold blocking job; cache hits and shared flights do
    /// not copy its text or the highlighted strings. When every job is taken, the lines
    /// come back unhighlighted (escaped as usual) instead of the page being refused.
    pub async fn highlight_lines_cached<T>(&self, content_id: &str, file_name: &str, text: T) -> WebResult<Arc<HighlightedFile>>
    where
        T: AsRef<str> + Send + 'static,
    {
        let (highlighter, name) = (self.clone(), file_name.to_owned());
        self.cache
            .get_or_compute(content_id, file_name, move |admitted| {
                if admitted { highlighter.highlight_lines(&name, text.as_ref()) } else { plain_lines(text.as_ref()) }
            })
            .await
    }

    /// How the cache of highlighted files is doing.
    pub fn cache_stats(&self) -> super::CacheStats {
        self.cache.stats()
    }

    /// Highlights `text` into one self-contained HTML fragment per line of flat spans, so
    /// each line can live in its own row.
    pub fn highlight_lines(&self, file_name: &str, text: &str) -> Vec<String> {
        let syntax = self.detect(file_name, text).filter(|_| text.len() <= MAX_HIGHLIGHT_BYTES);
        let Some(syntax) = syntax else {
            return plain_lines(text);
        };

        let set = self.syntaxes();
        let mut parser = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut resolver = Resolver::default();
        let mut lines = Vec::with_capacity(line_count(text));

        for line in LinesWithEndings::from(text) {
            if line.len() > MAX_LINE_LEN {
                return plain_lines(text);
            }
            let Ok(ops) = parser.parse_line(line, set) else {
                return plain_lines(text);
            };
            // Each line is rendered in its own row, so its terminator is cut off (a line
            // comment's scope includes it); anything else that breaks lines is shown.
            let end = content_len(line);
            let mut writer = LineWriter::with_capacity(end);
            for (range, op) in ScopeRangeIterator::new(&ops, line) {
                if stack.apply(op).is_err() {
                    return plain_lines(text);
                }
                let range = range.start.min(end)..range.end.min(end);
                if range.is_empty() {
                    continue;
                }
                let style = resolver.resolve(stack.as_slice());
                writer.push(style, &line[range]);
            }
            lines.push(writer.finish());
        }
        lines
    }
}

/// The length of `line` without its terminator, `\n` or `\r\n`, as [`str::lines`] cuts it.
fn content_len(line: &str) -> usize {
    line.strip_suffix('\n').map_or(line.len(), |line| line.strip_suffix('\r').unwrap_or(line).len())
}

/// How many lines [`str::lines`] and [`LinesWithEndings`] yield for `text`, to size the
/// output once.
fn line_count(text: &str) -> usize {
    text.bytes().filter(|&byte| byte == b'\n').count() + usize::from(!text.is_empty() && !text.ends_with('\n'))
}

fn plain_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::with_capacity(line_count(text));
    lines.extend(text.lines().map(escape_html));
    lines
}

/// The escaping syntect applies to highlighted text, so plain and highlighted lines agree.
fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        '\'' => out.push_str("&#39;"),
        other => out.push(other),
    }
}

/// Escapes one line for its row, showing what would break it (see [`super::row_char`]).
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        push_escaped(&mut out, super::row_char(ch));
    }
    out
}

#[cfg(test)]
mod css_oracle;

#[cfg(test)]
mod tests;
