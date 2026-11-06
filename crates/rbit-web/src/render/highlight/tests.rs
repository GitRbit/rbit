//! Compact syntax output parity, theme inheritance, escaping and cache contracts.

use std::fmt::Write;

use syntect::html::{ClassStyle, line_tokens_to_classed_spans};

use super::{
    css_oracle::{computed, computed_with_keyword_weight, parse_css},
    *,
};

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
    let css = include_str!("../../../styles/app.css");
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

#[test]
fn keyword_weight_fallback_preserves_inheritance_and_contrast_is_intentional() {
    let rules = parse_css(&compact_css());
    let keyword = "<span class=\"k\">use</span>";
    let default = computed(keyword, &rules);
    assert!(default.iter().all(|(_, style)| !style.iter().any(|(property, _)| property == "font-weight")));
    let contrast = computed_with_keyword_weight(keyword, &rules, Some("700"));
    assert!(contrast.iter().all(|(_, style)| style.contains(&("font-weight".into(), "700".into()))));

    let inherited = computed("<span class=\"b\"><span class=\"k\">x</span></span>", &rules);
    assert!(inherited[0].1.contains(&("font-weight".into(), "600".into())));
    let explicit = computed_with_keyword_weight("<span class=\"k b\">x</span>", &rules, Some("700"));
    assert!(explicit[0].1.contains(&("font-weight".into(), "600".into())), "explicit bold keeps source-order precedence");
    let typed = computed_with_keyword_weight("<span class=\"t\">fn</span>", &rules, Some("700"));
    assert!(!typed.iter().any(|(_, style)| style.iter().any(|(property, _)| property == "font-weight")));

    let css = include_str!("../../../styles/app.css");
    assert!(css.contains("--syntax-keyword-weight: initial;"));
    assert!(css.contains("--syntax-keyword-weight: 700;"));
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
    assert_equivalent("highlight.rs", include_str!("../highlight.rs"));
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
    assert_equivalent("app.css", include_str!("../../../styles/app.css"));
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
    let text = include_str!("../../routes/pulls.rs");
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
