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
//!
//! The sanitiser is configured once per process (a configured `ammonia::Builder` is
//! hundreds of allocations). Its attribute filter cannot carry per-document state, so the
//! document being sanitised on a thread is lent to it through a thread-local for the
//! duration of that one synchronous `clean` call ([`DocumentScope`]).
//!
//! Every `id` in the output (heading anchors, footnotes and raw HTML alike) is prefixed
//! with [`ID_PREFIX`], as GitHub does, so a document cannot claim an id the page itself
//! uses (such as `issuecomment-12`, which htmx targets); in-document `#fragment` links
//! are rewritten to match.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

use super::document_cache::DocumentCache;
use crate::urls::{DocumentBase, LinkKind, ResolvedLink};

/// The prefix of every `id` a rendered document carries.
pub const ID_PREFIX: &str = "user-content-";

/// Renders untrusted Markdown. The output is sanitised with ammonia, so it is safe to
/// insert into a page verbatim even when the input contains raw HTML.
///
/// With a `base`, relative links and images resolve against the document's location in
/// its repository (see [`DocumentBase::resolve`]); without one they are left as written.
/// Documents with a base (READMEs, Markdown files, release notes) are kept in
/// [`DocumentCache::shared`], so viewing one again copies the stored HTML instead of
/// rendering it.
pub fn render_markdown(source: &str, base: Option<&DocumentBase>) -> String {
    match base {
        Some(base) => DocumentCache::shared().get_or_render(source, base, || render(source, Some(base), None)),
        None => render(source, None, None),
    }
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
    let _scope = base.map(|base| DocumentScope::enter(Document { base: base.clone(), kept }));
    sanitizer().clean(&unsafe_html).to_string()
}

/// What the shared sanitiser resolves links against while it cleans one document.
struct Document {
    base: DocumentBase,
    /// Link targets of rewritten text, kept exactly as written.
    kept: HashSet<String>,
}

thread_local! {
    /// The document being sanitised on this thread, if it has a base.
    static DOCUMENT: RefCell<Option<Document>> = const { RefCell::new(None) };
}

/// Lends a document to [`filter_attribute`] until dropped, restoring what was there
/// before (also when sanitising panics).
struct DocumentScope {
    previous: Option<Document>,
}

impl DocumentScope {
    fn enter(document: Document) -> Self {
        Self { previous: DOCUMENT.replace(Some(document)) }
    }
}

impl Drop for DocumentScope {
    fn drop(&mut self) {
        DOCUMENT.set(self.previous.take());
    }
}

/// The shared sanitiser's attribute filter: with a document on this thread, relative
/// targets resolve against its base; without one, only `srcset` is checked.
fn filter_attribute<'u>(element: &str, attribute: &str, value: &'u str) -> Option<Cow<'u, str>> {
    DOCUMENT.with_borrow(|document| {
        if document.as_ref().is_some_and(|document| element == "a" && attribute == "href" && document.kept.contains(value)) {
            return Some(Cow::Borrowed(value));
        }
        if let Some(fragment) = prefixed_fragment(element, attribute, value) {
            return Some(Cow::Owned(fragment));
        }
        match document {
            Some(document) => resolve_attribute(&document.base, element, attribute, value),
            None if matches!(element, "source" | "img") && attribute == "srcset" => Some(Cow::Owned(resolve_srcset(None, value))),
            None => Some(Cow::Borrowed(value)),
        }
    })
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
    if matches!(element, "img" | "source") && attribute == "srcset" {
        return Some(Cow::Owned(resolve_srcset(Some(base), value)));
    }
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

/// `#x` → `#user-content-x` for in-document links, so they reach the prefixed ids.
fn prefixed_fragment(element: &str, attribute: &str, value: &str) -> Option<String> {
    let fragment = value.strip_prefix('#').filter(|fragment| !fragment.is_empty())?;
    (element == "a" && attribute == "href" && !fragment.starts_with(ID_PREFIX)).then(|| format!("#{ID_PREFIX}{fragment}"))
}

/// URLs in srcset may contain commas (notably data URLs), so a comma alone is not
/// a separator until after the URL token. Preserve density/width descriptors.
fn resolve_srcset(base: Option<&DocumentBase>, value: &str) -> String {
    let mut rest = value;
    let mut candidates = Vec::new();
    while !rest.is_empty() {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if rest.is_empty() {
            break;
        }
        let end = rest.find(|c: char| c.is_ascii_whitespace()).unwrap_or(rest.len());
        let token = &rest[..end];
        rest = &rest[end..];
        let url = token.trim_end_matches(',');
        let descriptor = if token.ends_with(',') {
            ""
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            let descriptor = rest[..end].trim();
            rest = &rest[end..];
            descriptor
        };
        // Unlike src, ammonia does not validate each URL inside srcset.
        let scheme = url.split_once(':').map(|(scheme, _)| scheme.to_ascii_lowercase());
        if scheme.as_deref().is_some_and(|scheme| !matches!(scheme, "http" | "https") && !scheme.contains('/')) {
            continue;
        }
        let resolved = match base.map_or(ResolvedLink::Unchanged, |base| base.resolve(url, LinkKind::Image)) {
            ResolvedLink::Unchanged => url.to_owned(),
            ResolvedLink::Rewritten(url) => url,
            ResolvedLink::Dropped => continue,
        };
        candidates.push(if descriptor.is_empty() { resolved } else { format!("{resolved} {descriptor}") });
    }
    candidates.join(", ")
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

/// How many sanitisers were configured, for tests: one per process.
#[cfg(test)]
static CONFIGURED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn configured_builder() -> ammonia::Builder<'static> {
    #[cfg(test)]
    CONFIGURED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(["input", "details", "summary", "picture", "source"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("source", ["srcset", "media", "type"])
        .add_tag_attributes("img", ["align", "width", "height"])
        .add_tag_attributes("img", ["srcset", "sizes"])
        .add_allowed_classes("code", ["language-mermaid"])
        .add_tag_attributes("p", ["align"])
        .add_tag_attributes("div", ["align"])
        .add_generic_attributes(["id"])
        .id_prefix(Some(ID_PREFIX))
        .link_rel(Some("noopener noreferrer nofollow"));
    builder
}

/// The one sanitiser, for every document; per-document state reaches its attribute filter
/// through [`DOCUMENT`].
fn sanitizer() -> &'static ammonia::Builder<'static> {
    static SANITIZER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    SANITIZER.get_or_init(|| {
        let mut builder = configured_builder();
        builder.attribute_filter(filter_attribute);
        builder
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urls::RepoUrls;

    fn in_docs() -> DocumentBase {
        DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "docs")
    }

    #[test]
    fn one_sanitiser_serves_every_document_and_forgets_each_base() {
        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..10 {
                        let html = render_markdown("[guide](guide.md) <img srcset=\"javascript:x 1x, a.png 2x\">", Some(&in_docs()));
                        assert!(html.contains(r#"href="/alice/proj/blob/main/docs/guide.md""#), "{html}");
                        assert!(!html.contains("javascript"), "{html}");
                        // The next document on this thread has no base: nothing of the last one's remains.
                        let html = render_markdown("[guide](guide.md)", None);
                        assert!(html.contains(r#"href="guide.md""#), "{html}");
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(CONFIGURED.load(std::sync::atomic::Ordering::Relaxed), 1, "configured once, not per document");
        DOCUMENT.with_borrow(|document| assert!(document.is_none()));
    }

    #[test]
    fn documents_with_a_base_are_rendered_once() {
        // Unique to this test, so other tests sharing the process cache cannot hit it.
        let source = format!("# Cached once\n\n{}", "A [relative link](guide.md) in a long README. ".repeat(30));
        let first = render_markdown(&source, Some(&in_docs()));
        let hits = DocumentCache::shared().stats().hits;
        assert_eq!(render_markdown(&source, Some(&in_docs())), first);
        assert!(DocumentCache::shared().stats().hits > hits, "the second view is a hit");
        assert!(first.contains(r#"href="/alice/proj/blob/main/docs/guide.md""#));
        // Another base resolves the same links elsewhere.
        let root = render_markdown(&source, Some(&DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "")));
        assert!(root.contains(r#"href="/alice/proj/blob/main/guide.md""#));
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
    fn picture_and_responsive_images_resolve_all_candidates() {
        let html = render_markdown(
            r#"<picture><source media="(prefers-color-scheme: dark)" srcset="../docs/dark.avif"><img src="light.avif" srcset="small.avif 1x, ../large%20image.avif 2x" sizes="100vw"></picture>"#,
            Some(&in_docs()),
        );
        assert!(html.contains(r#"srcset="/alice/proj/raw/main/docs/dark.avif""#), "{html}");
        assert!(html.contains(r#"srcset="/alice/proj/raw/main/docs/small.avif 1x, /alice/proj/raw/main/large%20image.avif 2x""#), "{html}");
        assert!(html.contains(r#"src="/alice/proj/raw/main/docs/light.avif""#), "{html}");
    }

    #[test]
    fn srcset_removes_unsafe_candidates_and_preserves_external_urls() {
        let source = r#"<source srcset="javascript:alert(1) 1x, https://cdn.example/a,b.avif 2x, ../../escape.avif 3x"><img srcset="data:text/html,bad 1x, safe.avif 2x">"#;
        for base in [None, Some(in_docs())] {
            let html = render_markdown(source, base.as_ref());
            assert!(!html.contains("javascript:"), "{html}");
            assert!(!html.contains("data:"), "{html}");
            assert!(html.contains("https://cdn.example/a,b.avif 2x"), "{html}");
            if base.is_some() {
                assert!(!html.contains("escape.avif"), "{html}");
            }
        }
    }

    #[test]
    fn mermaid_fences_keep_only_the_supported_class_and_escaped_source() {
        let html = render_markdown("```mermaid\ngraph TD\n A[<script>alert(1)</script>] --> B\n```\n\n```rust\nfn main() {}\n```", None);
        assert!(html.contains(r#"<code class="language-mermaid">"#), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(!html.contains("<script>"), "{html}");
        assert!(!html.contains("language-rust"), "{html}");
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
        assert!(html.contains(r##"href="#user-content-intro""##), "{html}");
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
        let anchor = |id: &str| format!(r#"<a id="user-content-{id}" rel="noopener noreferrer nofollow"></a>"#);
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
        assert!(html.contains(r#"<h2 id="user-content-mine">"#), "{html}");
        assert!(
            !render_markdown(
                "# !!!
", None
            )
            .contains("<a id"),
            "no empty anchors"
        );
    }

    #[test]
    fn cached_documents_keep_prefixed_ids_and_fragment_links() {
        let source = format!("<div id=\"issuecomment-1\">text</div>\n\n[comment](#issuecomment-1)\n\n{}", "Some text. ".repeat(100));
        let base = in_docs();
        let first = render_markdown(&source, Some(&base));
        let cached = render_markdown(&source, Some(&base));
        assert_eq!(first, cached);
        assert!(cached.contains(r#"id="user-content-issuecomment-1""#), "{cached}");
        assert!(cached.contains(r##"href="#user-content-issuecomment-1""##), "{cached}");
        assert!(!cached.contains(r#"id="issuecomment-1""#), "{cached}");
    }

    #[test]
    fn user_ids_cannot_collide_with_page_ids() {
        for base in [None, Some(in_docs())] {
            let html = render_markdown(
                "<div id=\"issuecomment-1\" name=\"x\">a</div> <span id=\"user-content-kept\"></span>\n\n[c](#issuecomment-1) [k](#user-content-kept)\n\nNote[^1]\n\n[^1]: Footnote.\n",
                base.as_ref(),
            );
            assert!(!html.contains(r#"id="issuecomment-1""#) && html.contains(r#"id="user-content-issuecomment-1""#), "{html}");
            assert!(html.contains(r##"href="#user-content-issuecomment-1""##), "links follow the prefixed ids: {html}");
            assert!(html.contains(r#"id="user-content-kept""#) && html.contains(r##"href="#user-content-kept""##), "{html}");
            assert!(!html.contains("name="), "{html}");
            assert!(!html.contains(r#" id="1""#) && !html.contains(r##"href="#1""##), "footnotes are prefixed too: {html}");
        }
    }
}
