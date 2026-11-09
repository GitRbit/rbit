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
//!    introduce markup it would reject. When the document has a [`LinkResolver`], the
//!    sanitiser's attribute filter also resolves relative `<a href>` and `<img src>`
//!    targets (from Markdown and from raw HTML alike) through it, after ammonia has
//!    already discarded dangerous schemes.
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
//!
//! Where relative links lead is the caller's decision: `rbit-web` implements
//! [`LinkResolver`] for a document's place in its repository, and keeps rendered
//! documents in its cache.

pub mod bounds;
mod headings;
mod links;

use std::{borrow::Cow, cell::RefCell, collections::HashSet, sync::OnceLock};

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

pub use links::{LinkKind, LinkResolver, ResolvedLink};

/// The prefix of every `id` a rendered document carries.
pub const ID_PREFIX: &str = "user-content-";

/// Renders untrusted Markdown. The output is sanitised with ammonia, so it is safe to
/// insert into a page verbatim even when the input contains raw HTML.
///
/// With a `base`, relative links and images resolve through it; without one they are left
/// as written.
pub fn render(source: &str, base: Option<&dyn LinkResolver>) -> String {
    render_with(source, base, None)
}

/// What [`render_rewriting`] puts in place of one run of text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rewritten {
    /// The replacement, as (untrusted, still sanitised) HTML.
    pub html: String,
    /// Link targets the replacement contains: they are kept exactly as written instead
    /// of being resolved against the document's base.
    pub hrefs: Vec<String>,
}

/// [`render`], with every run of plain text (adjacent text events joined; never inside
/// code, code blocks, links or images) offered to `rewrite`, which may return HTML to put
/// in its place. Sanitisation stays the last step.
pub fn render_rewriting(source: &str, base: Option<&dyn LinkResolver>, rewrite: &dyn Fn(&str) -> Option<Rewritten>) -> String {
    render_with(source, base, Some(rewrite))
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

fn render_with(source: &str, base: Option<&dyn LinkResolver>, rewrite: Option<Rewrite<'_>>) -> String {
    let mut events = headings::with_heading_ids(bounds::bounded(Parser::new_ext(source, options())));
    let mut kept: HashSet<String> = HashSet::new();
    if let Some(rewrite) = rewrite {
        events = rewrite_text(events, rewrite, &mut kept);
    }
    let mut unsafe_html = String::with_capacity(source.len() * 3 / 2);
    html::push_html(&mut unsafe_html, events.into_iter());
    let _scope = base.map(|base| DocumentScope::enter(Document { base: base.boxed(), kept }));
    sanitizer().clean(&unsafe_html).to_string()
}

/// What the shared sanitiser resolves links against while it cleans one document.
struct Document {
    base: Box<dyn LinkResolver>,
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
        if let Some(fragment) = links::prefixed_fragment(element, attribute, value) {
            return Some(Cow::Owned(fragment));
        }
        match document {
            Some(document) => links::resolve_attribute(document.base.as_ref(), element, attribute, value),
            None if matches!(element, "source" | "img") && attribute == "srcset" => Some(Cow::Owned(links::resolve_srcset(None, value))),
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
mod tests;
