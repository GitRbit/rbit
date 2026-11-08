//! Structural bounds on rendered Markdown, applied to the parser's events before any HTML
//! is written or sanitised.
//!
//! The sanitiser's HTML parser does work proportional to the depth of the open-element
//! stack for many elements it inserts, so a document that nests deeply (a megabyte of `>`
//! is a million nested quotes; `<div>` repeated is the raw-HTML equivalent) costs time
//! quadratic in its size: hours for a single 1 MiB file. Nesting gains a reader nothing
//! past a few dozen levels, so:
//!
//! * Markdown containers (quotes, lists, emphasis, links, …) deeper than [`MAX_NESTING`]
//!   are flattened: their start and end are dropped and their content is kept.
//! * Raw HTML that would leave more than [`MAX_RAW_HTML_DEPTH`] elements open (by a cheap
//!   estimate: void and optionally closed elements do not count) is shown as text instead.
//!
//! Both bounds keep the work of every later step linear in the document's size.

use pulldown_cmark::{CowStr, Event};

/// Deepest Markdown container nesting rendered as structure.
pub const MAX_NESTING: usize = 32;

/// Most raw HTML elements left open at once, by [`raw_depth`]'s estimate.
pub const MAX_RAW_HTML_DEPTH: usize = 64;

/// Elements that never stay open: void elements, and those whose end tag the HTML parser
/// supplies by itself when a sibling starts.
const NOT_NESTING: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr", "p", "li", "dt", "dd", "tr",
    "td", "th", "option", "optgroup", "rt", "rp",
];

/// `events` with the bounds above applied.
pub fn bounded<'a>(events: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    // Whether each open container was kept; containers below a dropped one are deeper still.
    let mut open: Vec<bool> = Vec::new();
    let mut raw_open = 0_usize;
    events.filter_map(move |event| match event {
        Event::Start(tag) => {
            let keep = open.len() < MAX_NESTING;
            open.push(keep);
            keep.then_some(Event::Start(tag))
        }
        Event::End(tag) => open.pop().unwrap_or(true).then_some(Event::End(tag)),
        Event::Html(html) => Some(raw(html, &mut raw_open, Event::Html)),
        Event::InlineHtml(html) => Some(raw(html, &mut raw_open, Event::InlineHtml)),
        other => Some(other),
    })
}

/// Raw HTML as itself while the elements it leaves open fit the budget, as text otherwise.
fn raw<'a>(html: CowStr<'a>, open: &mut usize, keep: fn(CowStr<'a>) -> Event<'a>) -> Event<'a> {
    let (peak, after) = raw_depth(&html, *open);
    if peak > MAX_RAW_HTML_DEPTH {
        return Event::Text(html);
    }
    *open = after;
    keep(html)
}

/// The most elements open at once while `html` is read after `open` were, and how many
/// are open after it. An estimate for budgeting only: comments, declarations and
/// self-closing tags open nothing; an end tag closes one.
pub fn raw_depth(html: &str, mut open: usize) -> (usize, usize) {
    let bytes = html.as_bytes();
    let mut peak = open;
    let mut index = 0;
    while let Some(offset) = bytes[index..].iter().position(|&byte| byte == b'<') {
        index += offset + 1;
        match bytes.get(index) {
            Some(b'!') if bytes[index..].starts_with(b"!--") => {
                index = html[index..].find("-->").map_or(bytes.len(), |end| index + end + 3);
            }
            Some(b'/') => open = open.saturating_sub(1),
            Some(byte) if byte.is_ascii_alphabetic() => {
                let name_end = bytes[index..].iter().position(|byte| !byte.is_ascii_alphanumeric()).map_or(bytes.len(), |end| index + end);
                let name = html[index..name_end].to_ascii_lowercase();
                let tag_end = bytes[name_end..].iter().position(|&byte| byte == b'>').map(|end| name_end + end);
                let self_closing = tag_end.is_some_and(|end| end > 0 && bytes[end - 1] == b'/');
                if !self_closing && !NOT_NESTING.contains(&name.as_str()) {
                    open += 1;
                    peak = peak.max(open);
                }
                index = name_end;
            }
            _ => {}
        }
    }
    (peak, open)
}

#[cfg(test)]
mod tests {
    use pulldown_cmark::{Options, Parser, Tag, TagEnd};

    use super::*;

    fn depth(events: &[Event<'_>]) -> usize {
        let (mut open, mut deepest) = (0_usize, 0);
        for event in events {
            match event {
                Event::Start(_) => {
                    open += 1;
                    deepest = deepest.max(open);
                }
                Event::End(_) => open -= 1,
                _ => {}
            }
        }
        deepest
    }

    #[test]
    fn deep_markdown_is_flattened_with_its_content_kept() {
        for source in [">".repeat(500) + " deep", "1. ".repeat(500) + "deep", "- ".repeat(500) + "deep"] {
            let events: Vec<_> = bounded(Parser::new_ext(&source, Options::all())).collect();
            assert!(depth(&events) <= MAX_NESTING, "{} levels", depth(&events));
            assert!(events.iter().any(|event| matches!(event, Event::Text(text) if text.contains("deep"))));
            let starts = events.iter().filter(|event| matches!(event, Event::Start(_))).count();
            assert_eq!(starts, events.iter().filter(|event| matches!(event, Event::End(_))).count(), "kept containers stay paired");
        }
        let shallow: Vec<_> = bounded(Parser::new("> > quote")).collect();
        assert!(matches!(shallow[0], Event::Start(Tag::BlockQuote(_))));
        assert!(matches!(shallow.last(), Some(Event::End(TagEnd::BlockQuote(_)))));
    }

    /// The fastest of three renderings of `source`, each given ten seconds at most.
    fn render_time(source: &str) -> std::time::Duration {
        (0..3)
            .map(|_| {
                let (source, (done, finished)) = (source.to_owned(), std::sync::mpsc::channel());
                std::thread::spawn(move || {
                    let started = std::time::Instant::now();
                    drop(super::super::markdown::render_markdown(&source, None));
                    let _ = done.send(started.elapsed());
                });
                finished.recv_timeout(std::time::Duration::from_secs(10)).expect("a hostile document rendered within ten seconds")
            })
            .min()
            .unwrap()
    }

    #[test]
    fn hostile_nesting_renders_in_linear_time() {
        for unit in [">", "1. ", "- > ", "<div>", "<b id=1>x", "<a><b><i><em><span>"] {
            let small = render_time(&unit.repeat(16 * 1024 / unit.len()));
            let large = render_time(&unit.repeat(64 * 1024 / unit.len()));
            // Four times the input: about four times the time when linear, sixteen when
            // quadratic. Renderings too fast to measure reliably pass.
            assert!(
                large < std::time::Duration::from_millis(50) || large < small * 10,
                "{unit:?}: 16 KiB took {small:?}, 64 KiB took {large:?}"
            );
        }
    }

    #[test]
    fn raw_html_beyond_the_depth_budget_becomes_text() {
        assert_eq!(raw_depth("<div><p>a<br><img src=x/><b>x</b></div>", 0), (2, 0));
        assert_eq!(raw_depth("<!-- <div> --><span/>", 3), (3, 3));
        let source = "<div>".repeat(MAX_RAW_HTML_DEPTH + 10);
        let events: Vec<_> = bounded(Parser::new(&source)).collect();
        assert!(events.iter().all(|event| !matches!(event, Event::Html(_))), "the over-deep block is text");
        let inline = format!("x {} y", "<b id=1>z".repeat(MAX_RAW_HTML_DEPTH + 10));
        let events: Vec<_> = bounded(Parser::new(&inline)).collect();
        let kept = events.iter().filter(|event| matches!(event, Event::InlineHtml(_))).count();
        assert_eq!(kept, MAX_RAW_HTML_DEPTH, "inline tags are kept up to the budget");
        let ordinary = "<details><summary>More</summary>\n\nText\n\n</details>";
        assert!(bounded(Parser::new(ordinary)).all(|event| !matches!(event, Event::Text(ref text) if text.contains('<'))));
    }
}
