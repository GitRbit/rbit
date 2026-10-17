//! GitHub-flavoured Markdown → sanitised HTML.
//!
//! Rendering happens in three steps:
//!
//! 1. pulldown-cmark parses the source; headings without an explicit `{#id}` get a
//!    stable, GitHub-style anchor (`## Getting started` → `getting-started`, duplicates
//!    numbered `-1`, `-2`, …), emitted as an empty `<a id>` right before the heading so
//!    the heading element itself renders exactly as before.
//! 2. The events are rendered to (untrusted) HTML.
//! 3. ammonia sanitises it — always the last step, so nothing added before it can
//!    introduce markup it would reject. When the document has a [`DocumentBase`], the
//!    sanitiser's attribute filter also resolves relative `<a href>` and `<img src>`
//!    targets (from Markdown and from raw HTML alike) to repository URLs, after
//!    ammonia has already discarded dangerous schemes.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

use crate::urls::{DocumentBase, LinkKind, ResolvedLink};

/// Renders untrusted Markdown. The output is sanitised with ammonia, so it is safe to
/// insert into a page verbatim even when the input contains raw HTML.
///
/// With a `base`, relative links and images resolve against the document's location in
/// its repository (see [`DocumentBase::resolve`]); without one they are left as written.
pub fn render_markdown(source: &str, base: Option<&DocumentBase>) -> String {
    render(source, base, None)
}

/// What [`render_markdown_rewriting`] puts in place of one run of text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rewritten {
    /// The replacement, as (untrusted, still sanitised) HTML.
    pub html: String,
    /// Link targets the replacement contains: they are kept exactly as written instead
    /// of being resolved against the document's base.
    pub hrefs: Vec<String>,
}

/// [`render_markdown`], with every run of plain text (adjacent text events joined; never
/// inside code, code blocks, links or images) offered to `rewrite`, which may return
/// HTML to put in its place. Sanitisation stays the last step.
pub fn render_markdown_rewriting(source: &str, base: Option<&DocumentBase>, rewrite: &dyn Fn(&str) -> Option<Rewritten>) -> String {
    render(source, base, Some(rewrite))
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
}

/// Replaces a run of text with HTML, or leaves it (`None`).
type Rewrite<'r> = &'r dyn Fn(&str) -> Option<Rewritten>;

fn render(source: &str, base: Option<&DocumentBase>, rewrite: Option<Rewrite<'_>>) -> String {
    let mut events = with_heading_ids(Parser::new_ext(source, options()));
    let mut kept: HashSet<String> = HashSet::new();
    if let Some(rewrite) = rewrite {
        events = rewrite_text(events, rewrite, &mut kept);
    }
    let mut unsafe_html = String::with_capacity(source.len() * 3 / 2);
    html::push_html(&mut unsafe_html, events.into_iter());
    match base {
        None => sanitizer().clean(&unsafe_html).to_string(),
        Some(base) => {
            let mut builder = configured_builder();
            let base = base.clone();
            builder.attribute_filter(move |element, attribute, value| {
                if element == "a" && attribute == "href" && kept.contains(value) {
                    return Some(Cow::Borrowed(value));
                }
                resolve_attribute(&base, element, attribute, value)
            });
            builder.clean(&unsafe_html).to_string()
        }
    }
}

/// Joins adjacent text events outside code, links and images and replaces each run with
/// what `rewrite` returns for it (if anything), remembering the replacements' links.
fn rewrite_text<'a>(events: Vec<Event<'a>>, rewrite: &dyn Fn(&str) -> Option<Rewritten>, kept: &mut HashSet<String>) -> Vec<Event<'a>> {
    let mut output: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut run: Vec<Event<'a>> = Vec::new();
    let mut hidden = 0_usize;
    let flush = |run: &mut Vec<Event<'a>>, output: &mut Vec<Event<'a>>, kept: &mut HashSet<String>| {
        if run.is_empty() {
            return;
        }
        let text: String = run.iter().filter_map(|event| if let Event::Text(text) = event { Some(text.as_ref()) } else { None }).collect();
        match rewrite(&text) {
            Some(rewritten) => {
                kept.extend(rewritten.hrefs);
                output.push(Event::InlineHtml(CowStr::from(rewritten.html)));
                run.clear();
            }
            None => output.append(run),
        }
    };
    for event in events {
        match &event {
            Event::Text(_) if hidden == 0 => {
                run.push(event);
                continue;
            }
            Event::Start(Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_)) => hidden += 1,
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => hidden = hidden.saturating_sub(1),
            _ => {}
        }
        flush(&mut run, &mut output, kept);
        output.push(event);
    }
    flush(&mut run, &mut output, kept);
    output
}

/// `text` with `&`, `<`, `>`, `"` and `'` escaped, for building HTML by hand.
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// Rewrites relative link and image targets; every other attribute passes unchanged.
fn resolve_attribute<'u>(base: &DocumentBase, element: &str, attribute: &str, value: &'u str) -> Option<Cow<'u, str>> {
    let kind = match (element, attribute) {
        ("a", "href") => LinkKind::Page,
        ("img", "src") => LinkKind::Image,
        _ => return Some(Cow::Borrowed(value)),
    };
    match base.resolve(value, kind) {
        ResolvedLink::Unchanged => Some(Cow::Borrowed(value)),
        ResolvedLink::Rewritten(url) => Some(Cow::Owned(url)),
        // Without its target the link renders as plain text (and the image as nothing).
        ResolvedLink::Dropped => None,
    }
}

/// Collects the events, adding an anchor named after its text before every heading that
/// has no explicit id.
fn with_heading_ids<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut output: Vec<Event<'a>> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    // Index of the open heading's start event and its text so far.
    let mut open: Option<(usize, String)> = None;
    for event in events {
        match &event {
            Event::Start(Tag::Heading { id: None, .. }) => open = Some((output.len(), String::new())),
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, heading_text)) = open.as_mut() {
                    heading_text.push_str(text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((index, text)) = open.take() {
                    let slug = unique_slug(&mut seen, &text);
                    if !slug.is_empty() {
                        // Slugs hold only letters, digits, `-` and `_`: nothing to escape.
                        output.insert(index, Event::Html(CowStr::from(format!("<a id=\"{slug}\"></a>"))));
                    }
                }
            }
            _ => {}
        }
        output.push(event);
    }
    output
}

/// GitHub's anchor slug: lower-case, letters, digits, `-` and `_` kept, spaces turned
/// into `-`, everything else removed; repeats get `-1`, `-2`, ….
fn unique_slug(seen: &mut HashMap<String, usize>, text: &str) -> String {
    let slug: String = text
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect();
    let count = seen.entry(slug.clone()).or_insert(0);
    let unique = if *count == 0 { slug.clone() } else { format!("{slug}-{count}") };
    *count += 1;
    unique
}

fn configured_builder() -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(["input", "details", "summary", "picture", "source"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("source", ["srcset", "media", "type"])
        .add_tag_attributes("img", ["align", "width", "height"])
        .add_tag_attributes("p", ["align"])
        .add_tag_attributes("div", ["align"])
        .add_generic_attributes(["id"])
        .link_rel(Some("noopener noreferrer nofollow"));
    builder
}

/// The shared sanitiser for documents without a base (no per-document filter).
fn sanitizer() -> &'static ammonia::Builder<'static> {
    static SANITIZER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    SANITIZER.get_or_init(configured_builder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urls::RepoUrls;

    fn in_docs() -> DocumentBase {
        DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "docs")
    }

    #[test]
    fn strips_scripts() {
        let html = render_markdown("hello <script>alert(1)</script> <img src=x onerror=alert(1)>", None);
        assert!(!html.contains("<script"));
        assert!(!html.contains("onerror"));
    }

    #[test]
    fn renders_tables_and_tasks() {
        let html = render_markdown("| a |\n|---|\n| b |\n\n- [x] done", None);
        assert!(html.contains("<table>"));
        assert!(html.contains("checkbox"));
    }

    #[test]
    fn relative_links_and_images_resolve_against_the_document() {
        let html =
            render_markdown("[guide](guide.md) [up](../README.md) ![logo](img/logo.png) [web](https://example.com)", Some(&in_docs()));
        assert!(html.contains(r#"href="/alice/proj/blob/main/docs/guide.md""#), "{html}");
        assert!(html.contains(r#"href="/alice/proj/blob/main/README.md""#), "{html}");
        assert!(html.contains(r#"src="/alice/proj/raw/main/docs/img/logo.png""#), "{html}");
        assert!(html.contains(r#"href="https://example.com""#), "{html}");
    }

    #[test]
    fn raw_html_links_and_images_resolve_too() {
        let html = render_markdown("<p align=\"center\"><img src=\"../logo.svg\"> <a href=\"setup.md\">Setup</a></p>", Some(&in_docs()));
        assert!(html.contains(r#"src="/alice/proj/raw/main/logo.svg""#), "{html}");
        assert!(html.contains(r#"href="/alice/proj/blob/main/docs/setup.md""#), "{html}");
    }

    #[test]
    fn links_escaping_the_root_become_plain_text() {
        let html = render_markdown("[secret](../../outside.md) and ![x](../../x.png)", Some(&in_docs()));
        assert!(!html.contains("href="), "{html}");
        assert!(!html.contains("src="), "{html}");
        assert!(html.contains("secret"), "the link text stays: {html}");
    }

    #[test]
    fn anchors_and_mail_links_are_untouched() {
        let html = render_markdown("[top](#intro) [mail](mailto:ada@example.com)", Some(&in_docs()));
        assert!(html.contains(r##"href="#intro""##), "{html}");
        assert!(html.contains(r#"href="mailto:ada@example.com""#), "{html}");
    }

    #[test]
    fn script_urls_are_still_removed_with_a_base() {
        let html = render_markdown("[x](javascript:alert(1)) <a href=\"JaVaScRiPt:alert(1)\">y</a>", Some(&in_docs()));
        assert!(!html.to_ascii_lowercase().contains("javascript"), "{html}");
    }

    #[test]
    fn headings_get_stable_unique_ids() {
        let html = render_markdown("# Getting started\n\n## Getting started\n\n### `cargo` & Rust!\n\n## Custom {#mine}\n", None);
        let anchor = |id: &str| format!(r#"<a id="{id}" rel="noopener noreferrer nofollow"></a>"#);
        assert!(
            html.contains(&format!(
                "{}
<h1>Getting started</h1>",
                anchor("getting-started")
            )),
            "{html}"
        );
        assert!(
            html.contains(&format!(
                "{}
<h2>",
                anchor("getting-started-1")
            )),
            "{html}"
        );
        assert!(
            html.contains(&format!(
                "{}
<h3>",
                anchor("cargo--rust")
            )),
            "{html}"
        );
        assert!(html.contains(r#"<h2 id="mine">"#), "{html}");
        assert!(
            !render_markdown(
                "# !!!
", None
            )
            .contains("<a id"),
            "no empty anchors"
        );
    }
}
