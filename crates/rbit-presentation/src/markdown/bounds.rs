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
//! Raw HTML is interpreted by an HTML5 parser, whose implicit elements, error recovery,
//! foreign content and raw-text states cannot be predicted by the cheap depth estimate.
//! Independently cap the total number of literal `<` bytes emitted as raw HTML. Every
//! raw start tag consumes one, so even syntax that evades the depth
//! estimate has a fixed structural budget. Raw active-formatting starts have a smaller
//! independent budget because the HTML5 parser can clone them across paragraphs.
//! Raw table/template contexts and raw active formatting are never admitted together:
//! foster parenting can reconstruct even one formatting attribute for every paragraph.
//! Excess raw events become escaped text;
//! ordinary Markdown and reasonably sized HTML tables keep their formatting.

use pulldown_cmark::{CowStr, Event};

/// Deepest Markdown container nesting rendered as structure.
pub const MAX_NESTING: usize = 32;

/// Most raw HTML elements left open at once, by [`raw_depth`]'s estimate.
pub const MAX_RAW_HTML_DEPTH: usize = 64;

/// Maximum raw markup introducers emitted in one document, regardless of HTML syntax.
/// Count all `<` bytes, even inside comments and attribute values: overestimating is
/// safe, while trying to recognize HTML here would recreate the parser's attack surface.
pub const MAX_RAW_HTML_MARKUP: usize = 4096;

/// HTML can reconstruct active formatting elements for every subsequent paragraph.
/// Keep their raw starts below a separate, smaller budget so the structural markup
/// budget cannot multiply into millions of parser-created nodes.
pub const MAX_RAW_FORMATTING: usize = 64;

/// Limit additional raw markup after formatting starts, including fragments and
/// closing tags. Table-context exclusion separately prevents attribute-copy growth
/// from generated Markdown; this counter is not an HTML attribute parser.
pub const MAX_RAW_FORMATTING_BYTES: usize = 4096;

#[derive(Debug, Default)]
struct RawBudget {
    depth: usize,
    markup: usize,
    formatting: usize,
    formatting_bytes: usize,
    table_context: bool,
}

const FORMATTING: &[&[u8]] =
    &[b"a", b"b", b"big", b"code", b"em", b"font", b"i", b"nobr", b"s", b"small", b"strike", b"strong", b"tt", b"u"];

const TABLE_CONTEXT: &[&[u8]] =
    &[b"table", b"caption", b"col", b"colgroup", b"tbody", b"td", b"tfoot", b"th", b"thead", b"tr", b"template"];

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
    let mut raw_budget = RawBudget::default();
    events.filter_map(move |event| match event {
        Event::Start(tag) => {
            let keep = open.len() < MAX_NESTING;
            open.push(keep);
            keep.then_some(Event::Start(tag))
        }
        Event::End(tag) => open.pop().unwrap_or(true).then_some(Event::End(tag)),
        Event::Html(html) => Some(raw(html, &mut raw_budget, Event::Html)),
        Event::InlineHtml(html) => Some(raw(html, &mut raw_budget, Event::InlineHtml)),
        other => Some(other),
    })
}

/// Raw HTML as itself while the elements it leaves open fit the budget, as text otherwise.
fn raw<'a>(html: CowStr<'a>, budget: &mut RawBudget, keep: fn(CowStr<'a>) -> Event<'a>) -> Event<'a> {
    let introducers = html.bytes().filter(|&byte| byte == b'<').count();
    if introducers > MAX_RAW_HTML_MARKUP - budget.markup {
        return Event::Text(html);
    }
    let formatting_starts = formatting_starts(&html);
    if budget.table_context && formatting_starts > 0 {
        return Event::Text(html);
    }
    let table_context = !budget.table_context && possible_tags(&html, TABLE_CONTEXT, true) > 0;
    if table_context && (budget.formatting > 0 || formatting_starts > 0) {
        return Event::Text(html);
    }
    if formatting_starts > MAX_RAW_FORMATTING - budget.formatting {
        return Event::Text(html);
    }
    let formatting_bytes = if budget.formatting > 0 || formatting_starts > 0 { html.len() } else { 0 };
    if formatting_bytes > MAX_RAW_FORMATTING_BYTES - budget.formatting_bytes {
        return Event::Text(html);
    }
    let (peak, after) = raw_depth(&html, budget.depth);
    if peak > MAX_RAW_HTML_DEPTH {
        return Event::Text(html);
    }
    budget.depth = after;
    budget.markup += introducers;
    budget.formatting += formatting_starts;
    budget.formatting_bytes += formatting_bytes;
    budget.table_context |= table_context;
    keep(html)
}

/// Count possible formatting starts without parsing HTML: count even matches in
/// attributes/comments, and ignore closing tags. Only ASCII names followed by an HTML
/// tag delimiter can activate formatting. A partial name at an event boundary is
/// counted conservatively too. No stack, allocations or syntax-dependent refunds.
fn formatting_starts(html: &str) -> usize {
    possible_tags(html, FORMATTING, false)
}

/// Count possible names conservatively, even inside comments/attributes. Table end
/// tags are included because raw HTML must not manipulate a generated table's scope.
fn possible_tags(html: &str, names: &[&[u8]], include_closing: bool) -> usize {
    let bytes = html.as_bytes();
    bytes
        .iter()
        .enumerate()
        .filter(|&(index, &byte)| {
            if byte != b'<' {
                return false;
            }
            let mut tail = &bytes[index + 1..];
            if include_closing && tail.starts_with(b"/") {
                tail = &tail[1..];
            }
            let end = tail.iter().position(|byte| !byte.is_ascii_alphabetic()).unwrap_or(tail.len());
            let name = &tail[..end];
            if end == tail.len() {
                return names.iter().any(|candidate| candidate.get(..name.len()).is_some_and(|prefix| name.eq_ignore_ascii_case(prefix)));
            }
            matches!(tail[end], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'/' | b'>')
                && names.iter().any(|candidate| name.eq_ignore_ascii_case(candidate))
        })
        .count()
}

/// The most elements open at once while `html` is read after `open` were, and how many
/// are open after it. An estimate for budgeting only: comments, declarations and
/// self-closing tags open nothing; an end tag closes one.
pub fn raw_depth(html: &str, mut open: usize) -> (usize, usize) {
    let bytes = html.as_bytes();
    let mut peak = open;
    let mut index = 0;
    // Reuse the next delimiter: malformed tags containing many `<p ` fragments
    // before a distant `>` must not rescan the same suffix for each fragment.
    let mut tag_end = bytes.iter().position(|&byte| byte == b'>');
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
                if tag_end.is_some_and(|end| end < name_end) {
                    tag_end = bytes[name_end..].iter().position(|&byte| byte == b'>').map(|end| name_end + end);
                }
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
mod tests;
