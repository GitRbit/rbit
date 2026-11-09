//! Markdown text projection for reference scanning (`applications` only).
//!
//! Issue and pull bodies, comments and merge messages are Markdown. Their references are
//! found in the text a reader sees: adjacent text events are joined, code spans, code
//! blocks and link or image text are skipped, and text between raw HTML tags still counts.
//! Each joined run goes through the feature-owned [`Scan`]. The pure model graph has no
//! Markdown engine; only the `applications` feature adds pulldown-cmark.
//!
//! The options must stay identical to the ones safe rendering uses, so both see the same
//! text events; the rewrite check compares the two option sets.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::Scan;

/// The Markdown options issue texts are rendered with (shared with the renderer so both
/// see the same text events).
pub fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
}

/// The references in the text of Markdown `source`: adjacent text events are joined (so
/// `**Fixes** #1` still closes), code spans, code blocks and link or image text are skipped (text
/// between raw HTML tags still counts).
pub fn scan_markdown(source: &str) -> Scan {
    let mut scan = Scan::default();
    let mut buffer = String::new();
    let mut hidden = 0_usize;
    for event in Parser::new_ext(source, markdown_options()) {
        match event {
            Event::Text(text) if hidden == 0 => buffer.push_str(&text),
            Event::SoftBreak | Event::HardBreak if hidden == 0 => buffer.push('\n'),
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough)
            | Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {}
            Event::Start(Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_)) => {
                scan.add_text(&std::mem::take(&mut buffer));
                hidden += 1;
            }
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => {
                hidden = hidden.saturating_sub(1);
            }
            _ => scan.add_text(&std::mem::take(&mut buffer)),
        }
    }
    scan.add_text(&buffer);
    scan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{references::RemoteRef, threads::IssueNumber};

    fn n(value: u32) -> IssueNumber {
        IssueNumber::new(value).unwrap()
    }

    fn remote(owner: &str, repo: &str, number: u32) -> RemoteRef {
        RemoteRef { owner: owner.to_owned(), repo: repo.to_owned(), number: n(number) }
    }

    #[test]
    fn markdown_scans_skip_code_and_link_text() {
        let source = "Fixes #1, see #2 and @alice.\n\n`#3 @bob` and\n\n```\n#4 @carol\n```\n\n[#5 @dave](https://example.com) **Closes** #6 acme/web#9 <b>#7</b>\n";
        let scan = scan_markdown(source);
        assert_eq!(scan.local, [n(1), n(2), n(6), n(7)]);
        assert_eq!(scan.closes, [n(1), n(6)]);
        assert_eq!(scan.mentions, ["alice"]);
        assert_eq!(scan.remote, [remote("acme", "web", 9)]);
    }
}
