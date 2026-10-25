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
    /// not copy its text or the highlighted strings.
    pub async fn highlight_lines_cached<T>(&self, content_id: &str, file_name: &str, text: T) -> WebResult<Arc<HighlightedFile>>
    where
        T: AsRef<str> + Send + 'static,
    {
        let highlighter = self.clone();
        let name = file_name.to_owned();
        self.cache.get_or_compute(content_id, file_name, move || highlighter.highlight_lines(&name, text.as_ref())).await
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
mod tests {
    use std::fmt::Write;

    use syntect::html::{ClassStyle, line_tokens_to_classed_spans};

    use super::*;

    /// The stylesheet rules the nested markup was styled with before compact output.
    const NESTED_CSS: &str = r"
.hl-comment { color: var(--syntax-comment); }
.hl-string, .hl-markup.hl-raw { color: var(--syntax-string); }
.hl-constant.hl-numeric, .hl-constant.hl-language, .hl-constant.hl-character { color: var(--syntax-number); }
.hl-keyword, .hl-storage { color: var(--syntax-keyword); }
.hl-entity.hl-name.hl-function, .hl-support.hl-function, .hl-meta.hl-function-call { color: var(--syntax-function); }
.hl-entity.hl-name.hl-type, .hl-entity.hl-name.hl-class, .hl-support.hl-type, .hl-support.hl-class,
.hl-storage.hl-type { color: var(--syntax-type); }
.hl-entity.hl-name.hl-tag { color: var(--syntax-tag); }
.hl-entity.hl-other.hl-attribute-name { color: var(--syntax-attribute); }
.hl-markup.hl-heading { color: var(--ink); font-weight: 600; }
.hl-markup.hl-bold { font-weight: 600; }
.hl-markup.hl-italic { font-style: italic; }
.hl-invalid { color: var(--danger); }
";

    /// The **Syntax highlighting** section of the real stylesheet.
    fn compact_css() -> String {
        let css = include_str!("../../styles/app.css");
        let marker = "Syntax highlighting */";
        let start = css.find(marker).expect("section header") + marker.len();
        let rest = &css[start..];
        let end = rest.find("/* ----").expect("next section");
        // The rules are scoped to highlighted code; the resolver below matches classes on
        // one element, so it reads them without that (zero-specificity) scope.
        rest[..end].replace(":where(.line-code) ", "")
    }

    /// The nested renderer compact output replaced, kept to prove equivalence.
    fn nested_lines(highlighter: &Highlighter, file_name: &str, text: &str) -> Vec<String> {
        let syntax = highlighter.detect(file_name, text).expect("a syntax");
        let set = highlighter.syntaxes();
        let mut parser = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut lines = Vec::new();
        for line in LinesWithEndings::from(text) {
            let ops = parser.parse_line(line, set).unwrap();
            let mut html = String::new();
            for scope in stack.as_slice() {
                html.push_str("<span class=\"");
                for (i, atom) in scope.build_string().split('.').enumerate() {
                    if i > 0 {
                        html.push(' ');
                    }
                    let _ = write!(html, "hl-{atom}");
                }
                html.push_str("\">");
            }
            let (spans, _) = line_tokens_to_classed_spans(line, &ops, ClassStyle::SpacedPrefixed { prefix: "hl-" }, &mut stack).unwrap();
            html.extend(spans.chars().filter(|c| !matches!(c, '\n' | '\r')));
            for _ in 0..stack.len() {
                html.push_str("</span>");
            }
            lines.push(html);
        }
        lines
    }

    /// One parsed CSS rule: alternative class lists, its position, its declarations.
    struct CssRule {
        selectors: Vec<Vec<String>>,
        order: usize,
        declarations: Vec<(String, String)>,
    }

    fn parse_css(css: &str) -> Vec<CssRule> {
        let without_comments: String = {
            let mut out = String::new();
            let mut rest = css;
            while let Some(start) = rest.find("/*") {
                out.push_str(&rest[..start]);
                rest = rest[start..].find("*/").map_or("", |end| &rest[start + end + 2..]);
            }
            out.push_str(rest);
            out
        };
        let mut rules = Vec::new();
        for (order, chunk) in without_comments.split('}').enumerate() {
            let Some((selectors, body)) = chunk.split_once('{') else { continue };
            let selectors = selectors
                .split(',')
                .map(|selector| selector.split('.').map(str::trim).filter(|class| !class.is_empty()).map(str::to_owned).collect())
                .collect();
            let declarations = body
                .split(';')
                .filter_map(|declaration| declaration.split_once(':'))
                .map(|(property, value)| (property.trim().to_owned(), value.trim().to_owned()))
                .collect();
            rules.push(CssRule { selectors, order, declarations });
        }
        rules
    }

    /// The value each property gets on an element with `classes`, by specificity then order.
    fn declared(rules: &[CssRule], classes: &[&str]) -> HashMap<String, String> {
        let mut winners: HashMap<String, ((usize, usize), String)> = HashMap::new();
        for rule in rules {
            for selector in &rule.selectors {
                if !selector.iter().all(|class| classes.contains(&class.as_str())) {
                    continue;
                }
                let rank = (selector.len(), rule.order);
                for (property, value) in &rule.declarations {
                    if winners.get(property).is_none_or(|(best, _)| rank >= *best) {
                        winners.insert(property.clone(), (rank, value.clone()));
                    }
                }
            }
        }
        winners.into_iter().map(|(property, (_, value))| (property, value)).collect()
    }

    /// Every character of a line of markup (entities decoded) with its computed style.
    fn computed(html: &str, rules: &[CssRule]) -> Vec<(char, Vec<(String, String)>)> {
        let mut stack: Vec<HashMap<String, String>> = Vec::new();
        let mut out = Vec::new();
        let mut rest = html;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("</span>") {
                assert!(stack.pop().is_some(), "unbalanced close in {html}");
                rest = after;
            } else if let Some(after) = rest.strip_prefix("<span class=\"") {
                let (classes, after) = after.split_once("\">").expect("tag end");
                stack.push(declared(rules, &classes.split(' ').collect::<Vec<_>>()));
                rest = after;
            } else {
                assert!(!rest.starts_with('<'), "unexpected markup in {html}");
                let (ch, len) = [("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>'), ("&quot;", '"'), ("&#39;", '\'')]
                    .iter()
                    .find(|(entity, _)| rest.starts_with(entity))
                    .map_or_else(
                        || {
                            let ch = rest.chars().next().unwrap();
                            (ch, ch.len_utf8())
                        },
                        |(entity, ch)| (*ch, entity.len()),
                    );
                let mut style: Vec<(String, String)> = ["color", "font-weight", "font-style"]
                    .iter()
                    .filter_map(|property| {
                        stack.iter().rev().find_map(|element| element.get(*property)).map(|value| ((*property).to_owned(), value.clone()))
                    })
                    .collect();
                style.sort();
                out.push((ch, style));
                rest = &rest[len..];
            }
        }
        assert!(stack.is_empty(), "unclosed span in {html}");
        out
    }

    fn assert_equivalent(file_name: &str, text: &str) {
        let highlighter = Highlighter::new();
        let nested = nested_lines(&highlighter, file_name, text);
        let compact = highlighter.highlight_lines(file_name, text);
        assert_eq!(nested.len(), compact.len(), "{file_name}: line count");
        let (old_rules, new_rules) = (parse_css(NESTED_CSS), parse_css(&compact_css()));
        let mut styled = 0;
        for (number, (old, new)) in nested.iter().zip(&compact).enumerate() {
            let (old, new) = (computed(old, &old_rules), computed(new, &new_rules));
            assert_eq!(old, new, "{file_name}: line {} differs", number + 1);
            styled += new.iter().filter(|(_, style)| !style.is_empty()).count();
        }
        assert!(styled > 0, "{file_name}: nothing was styled");
    }

    const JS: &str = r#"// Fetch and render.
import { thing } from "./thing.js";
const answer = 42, pi = 3.14, flag = true;
/** Doc comment with <b>markup</b> & entities. */
export async function load(url) {
  const re = /a+b?/gi;
  const text = `value: ${answer * 2} 'quoted'`;
  if (url == null || url === "") throw new Error("missing");
  class Box extends Array { get size() { return this.length; } }
  return fetch(url).then((r) => r.json());
}
"#;

    const HTML: &str = r#"<!doctype html>
<html lang="en">
<!-- a comment -->
<head><title>T &amp; U</title>
<style>body { color: red; margin: 0 }</style>
<script>let x = "<b>" + 1;</script>
</head>
<body class="main" data-x='1'><p>Hello &lt;world&gt;</p><img src="a.png" alt=""></body>
</html>
"#;

    const MARKDOWN: &str = r#"# Heading *with emphasis*

Some **bold**, _italic_, ***both*** and `code` with a [link](https://example.com "title").

## Second `heading`

- item one
- item **two**

```rust
fn main() { println!("hi"); }
```

> quote with *emphasis*
"#;

    const CSS: &str = r#"/* Tokens */
:root { --gap: 4px; }
@media (max-width: 600px) {
  .card > a:hover, #main::before { content: "\201C"; color: #fff !important; margin: calc(1rem + 2px) 0; }
}
@import url("x.css");
"#;

    #[test]
    fn compact_output_looks_like_the_nested_markup_rust() {
        assert_equivalent("highlight.rs", include_str!("highlight.rs"));
        assert_equivalent("lib.rs", "#[derive(Debug)]\npub struct A<'a> { b: &'a str, c: u8 }\nlet s = \"x\\n\"; // c\n/* a\n b */ 0x1F\n");
    }

    #[test]
    fn compact_output_looks_like_the_nested_markup_javascript() {
        assert_equivalent("app.js", JS);
    }

    #[test]
    fn compact_output_looks_like_the_nested_markup_html() {
        assert_equivalent("index.html", HTML);
    }

    #[test]
    fn compact_output_looks_like_the_nested_markup_markdown() {
        assert_equivalent("README.md", MARKDOWN);
    }

    #[test]
    fn compact_output_looks_like_the_nested_markup_css() {
        assert_equivalent("site.css", CSS);
        assert_equivalent("app.css", include_str!("../../styles/app.css"));
    }

    #[test]
    fn lines_are_flat_and_balanced() {
        let lines = Highlighter::new().highlight_lines("main.rs", "/* multi\nline */\nfn main() { let s = \"<&>\"; }\n");
        assert_eq!(lines.len(), 3);
        for line in &lines {
            let mut depth = 0_i32;
            for (index, _) in line.match_indices('<') {
                if line[index..].starts_with("</span>") {
                    depth -= 1;
                } else {
                    assert!(line[index..].starts_with("<span class=\""), "only spans: {line}");
                    depth += 1;
                }
                assert!((0..=1).contains(&depth), "nested or unbalanced: {line}");
            }
            assert_eq!(depth, 0, "unbalanced: {line}");
        }
        assert_eq!(lines[0], "<span class=\"c\">/* multi</span>");
        // `fn` is `storage.type`: the type rule is more specific than the keyword rule.
        assert!(lines[2].contains("<span class=\"t\">fn</span>"), "{}", lines[2]);
        assert!(lines[2].contains("<span class=\"k\">=</span>"), "{}", lines[2]);
        assert!(lines[2].contains("&lt;&amp;&gt;"), "escaped: {}", lines[2]);
    }

    #[test]
    fn adjacent_runs_with_one_style_are_merged() {
        // The quotes and the text of a string are separate scopes but one colour.
        let lines = Highlighter::new().highlight_lines("a.rs", "let s = \"abc\";\n");
        assert!(lines[0].contains("<span class=\"s\">&quot;abc&quot;</span>"), "{}", lines[0]);
    }

    #[test]
    fn markup_styles_combine() {
        let lines = Highlighter::new().highlight_lines("a.md", "# Title *x*\n\n***both***\n");
        assert!(lines[0].starts_with("<span class=\"h\">"), "{}", lines[0]);
        assert!(lines[0].contains("<span class=\"h i\">"), "{}", lines[0]);
        assert!(lines[2].contains("class=\"b i\""), "{}", lines[2]);
    }

    #[test]
    fn compact_output_is_at_least_three_times_smaller() {
        let highlighter = Highlighter::new();
        let text = include_str!("../routes/pulls.rs");
        let nested: usize = nested_lines(&highlighter, "pulls.rs", text).iter().map(String::len).sum();
        let compact: usize = highlighter.highlight_lines("pulls.rs", text).iter().map(String::len).sum();
        assert!(nested >= compact * 3, "nested {nested} bytes, compact {compact} bytes");
    }

    #[tokio::test]
    async fn cached_highlighting_is_reused_per_blob_and_name() {
        let highlighter = Highlighter::new();
        let text = "fn main() {}\n";
        let first = highlighter.highlight_lines_cached("blob1", "a.rs", text).await.unwrap();
        let again = highlighter.highlight_lines_cached("blob1", "a.rs", "ignored: the id names the text").await.unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!(first.lines(), highlighter.highlight_lines("a.rs", text));
        let as_text = highlighter.highlight_lines_cached("blob1", "a.txt", text).await.unwrap();
        assert_eq!(as_text.lines(), ["fn main() {}"], "another name may pick another syntax");
        let stats = highlighter.cache_stats();
        assert_eq!((stats.hits, stats.misses, stats.entries), (1, 2, 2));
    }

    #[test]
    fn line_breaks_inside_a_line_are_shown_not_followed() {
        let highlighter = Highlighter::new();
        let unknown = highlighter.highlight_lines("notes.unknownext", "foo\rbar\r\nnext\u{2028}line\n");
        assert_eq!(unknown, ["foo\u{240D}bar", "next\u{2424}line"], "the CRLF terminator is cut, the lone CR shown");
        let rust = highlighter.highlight_lines("main.rs", "// foo\rbar\r\nlet x\u{0C} = 1;\n");
        assert_eq!(rust.len(), 2);
        assert!(rust[0].contains("foo\u{240D}bar") && !rust[0].contains(['\r', '\n']), "{}", rust[0]);
        assert!(rust[1].contains('\u{240C}') && !rust[1].contains('\u{0C}'), "{}", rust[1]);
    }

    #[test]
    fn the_class_table_spells_every_style_as_classes_joined_in_order() {
        let colors = [
            None,
            Some(Color::Comment),
            Some(Color::String),
            Some(Color::Number),
            Some(Color::Keyword),
            Some(Color::Function),
            Some(Color::Type),
            Some(Color::Tag),
            Some(Color::Attribute),
            Some(Color::Heading),
            Some(Color::Invalid),
        ];
        for color in colors {
            for (bold, italic) in [(false, false), (false, true), (true, false), (true, true)] {
                // How spans were spelled before the table: a vector joined per span.
                let mut expected: Vec<&str> = color.map(Color::class).into_iter().collect();
                if bold && color != Some(Color::Heading) {
                    expected.push("b");
                }
                if italic {
                    expected.push("i");
                }
                let expected = (!expected.is_empty()).then(|| expected.join(" "));
                assert_eq!(Style { color, bold, italic }.classes().map(str::to_owned), expected, "{color:?} {bold} {italic}");
            }
        }
    }

    #[test]
    fn line_counts_size_the_output_exactly() {
        for text in ["", "a", "a\n", "a\nb", "a\r\nb\r\n", "\n\n", "x\ny\n\n"] {
            assert_eq!(line_count(text), text.lines().count(), "{text:?}");
            assert_eq!(plain_lines(text).capacity(), text.lines().count(), "{text:?}: one allocation, no spare");
        }
        let text = "fn a() {}\nfn b() {}\n";
        let lines = Highlighter::new().highlight_lines("a.rs", text);
        assert_eq!(lines.capacity(), lines.len());
    }

    #[test]
    fn unknown_files_are_escaped_plain_text() {
        let lines = Highlighter::new().highlight_lines("notes.unknownext", "<b>hi</b>");
        assert_eq!(lines, vec!["&lt;b&gt;hi&lt;/b&gt;"]);
    }
}
