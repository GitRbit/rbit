//! References in Markdown and commit messages: `#12`, `owner/repo#12`, `@name` and
//! closing keywords (`fixes #12`). Pure parsing; resolution happens in the services.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! * [`find`] locates every reference in one piece of plain text, with its byte range,
//!   so a renderer can replace exactly those characters.
//! * [`scan_markdown`] collects the references of a Markdown document, looking only at
//!   its text: code spans, code blocks, raw HTML tags and link text are ignored.
//! * [`scan_plain`] does the same for text that is not Markdown (commit messages).
//!
//! The rules, close to GitHub's:
//! * a reference starts at the beginning of the text or after a character that cannot
//!   be part of a word (`abc#1`, `a@b` and `x/y#1z` are not references);
//! * numbers are canonical decimals in `1..=u32::MAX` (`#0`, `#012` and overlong numbers
//!   are ignored) and end at a non-word character;
//! * `@name` must be a valid account name (a trailing `.` or `-` is punctuation);
//! * a closing keyword (`close`, `closes`, `closed`, `fix`, `fixes`, `fixed`, `resolve`,
//!   `resolves`, `resolved`, any case), optionally followed by `:`, then whitespace, then
//!   a same-repository `#N`, marks that issue as closed by the text.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{AccountName, IssueNumber, RepoName};

/// At most this many distinct mentions of one text are honoured.
pub const MAX_MENTIONS: usize = 50;

/// Words that, before a `#N`, close issue `N`.
pub const CLOSING_KEYWORDS: [&str; 9] = ["close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved"];

/// What a reference points at.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Reference {
    /// `#12`: an issue or pull request of the same repository.
    Local(IssueNumber),
    /// `owner/repo#12`.
    Remote(RemoteRef),
    /// `@name`, as written (compare case-insensitively).
    Mention(String),
}

/// `owner/repo#12`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteRef {
    pub owner: String,
    pub repo: String,
    pub number: IssueNumber,
}

impl RemoteRef {
    /// Whether it names `owner/repo` (names compare case-insensitively).
    pub fn is_repository(&self, owner: &str, repo: &str) -> bool {
        self.owner.eq_ignore_ascii_case(owner) && self.repo.eq_ignore_ascii_case(repo)
    }
}

/// A reference found in a text: `text[start..end]` is exactly what was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub start: usize,
    pub end: usize,
    pub reference: Reference,
    /// A closing keyword precedes it (only for [`Reference::Local`]).
    pub closes: bool,
}

/// The distinct references of a whole text, in order of first appearance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    pub local: Vec<IssueNumber>,
    pub remote: Vec<RemoteRef>,
    /// Lower-cased, at most [`MAX_MENTIONS`].
    pub mentions: Vec<String>,
    /// Same-repository numbers after a closing keyword.
    pub closes: Vec<IssueNumber>,
}

impl Scan {
    fn add(&mut self, found: Found) {
        match found.reference {
            Reference::Local(number) => {
                push_unique(&mut self.local, number);
                if found.closes {
                    push_unique(&mut self.closes, number);
                }
            }
            Reference::Remote(remote) => push_unique(&mut self.remote, remote),
            Reference::Mention(name) => {
                let name = name.to_ascii_lowercase();
                if self.mentions.len() < MAX_MENTIONS {
                    push_unique(&mut self.mentions, name);
                }
            }
        }
    }

    fn add_text(&mut self, text: &str) {
        for found in find(text) {
            self.add(found);
        }
    }

    /// Merges `other` into this scan (for several texts of one thread).
    pub fn extend(&mut self, other: Self) {
        for number in other.local {
            push_unique(&mut self.local, number);
        }
        for remote in other.remote {
            push_unique(&mut self.remote, remote);
        }
        for name in other.mentions {
            if self.mentions.len() < MAX_MENTIONS {
                push_unique(&mut self.mentions, name);
            }
        }
        for number in other.closes {
            push_unique(&mut self.closes, number);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.local.is_empty() && self.remote.is_empty() && self.mentions.is_empty()
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

/// Characters that continue a word: a reference may not start right after one.
const fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Characters of account and repository names.
const fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')
}

/// Whether a reference may start at byte `at` of `text`.
fn starts_word(text: &str, at: usize) -> bool {
    text[..at].chars().next_back().is_none_or(|c| !is_word(c) && !matches!(c, '/' | '.' | '-' | '@' | '#' | '&' | '`'))
}

/// The canonical number in `digits` (all ASCII digits), or `None`.
fn number(digits: &str) -> Option<IssueNumber> {
    IssueNumber::parse(digits).ok()
}

/// Every reference in `text`, in order. `text` is plain: no Markdown is interpreted.
pub fn find(text: &str) -> Vec<Found> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                if let Some(reference) = hash_reference(text, i) {
                    i = reference.end;
                    found.push(reference);
                    continue;
                }
            }
            b'@' => {
                if let Some(mention) = mention(text, i) {
                    i = mention.end;
                    found.push(mention);
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    found
}

/// The digits after `#` at `hash`, if they form a number that ends at a word boundary.
fn digits_after(text: &str, hash: usize) -> Option<(IssueNumber, usize)> {
    let rest = &text[hash + 1..];
    let len = rest.bytes().take_while(u8::is_ascii_digit).count();
    if len == 0 || rest[len..].chars().next().is_some_and(is_word) {
        return None;
    }
    Some((number(&rest[..len])?, hash + 1 + len))
}

/// `#N` or `owner/repo#N` around the `#` at byte `hash`.
fn hash_reference(text: &str, hash: usize) -> Option<Found> {
    let (number, end) = digits_after(text, hash)?;
    let bytes = text.as_bytes();
    if hash == 0 || !is_name_char(bytes[hash - 1]) {
        if !starts_word(text, hash) {
            return None;
        }
        let closes = closing_keyword_before(text, hash);
        return Some(Found { start: hash, end, reference: Reference::Local(number), closes });
    }
    // `owner/repo#N`: walk back over the repository name, a slash and the owner.
    let repo_start = text[..hash].bytes().rposition(|b| !is_name_char(b)).map_or(0, |p| p + 1);
    if repo_start == 0 || bytes[repo_start - 1] != b'/' {
        return None;
    }
    let slash = repo_start - 1;
    let owner_start = text[..slash].bytes().rposition(|b| !is_name_char(b)).map_or(0, |p| p + 1);
    if !starts_word(text, owner_start) {
        return None;
    }
    let owner = &text[owner_start..slash];
    let repo = &text[repo_start..hash];
    owner.parse::<AccountName>().ok()?;
    repo.parse::<RepoName>().ok()?;
    let remote = RemoteRef { owner: owner.to_owned(), repo: repo.to_owned(), number };
    Some(Found { start: owner_start, end, reference: Reference::Remote(remote), closes: false })
}

/// Whether the text right before `at` is a closing keyword, an optional `:` and whitespace.
fn closing_keyword_before(text: &str, at: usize) -> bool {
    let before = &text[..at];
    let trimmed = before.trim_end();
    if trimmed.len() == before.len() {
        // `fixes:#1` (no space) also counts, `fixes#1` does not.
        let Some(word) = before.strip_suffix(':') else { return false };
        return keyword_at_end(word);
    }
    keyword_at_end(trimmed.strip_suffix(':').unwrap_or(trimmed))
}

fn keyword_at_end(text: &str) -> bool {
    let start = text.char_indices().rev().take_while(|(_, c)| c.is_ascii_alphabetic()).last().map(|(i, _)| i);
    let Some(start) = start else { return false };
    let word = &text[start..];
    text[..start].chars().next_back().is_none_or(|c| !is_word(c)) && CLOSING_KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(word))
}

/// `@name` at byte `at`.
fn mention(text: &str, at: usize) -> Option<Found> {
    if !starts_word(text, at) {
        return None;
    }
    let rest = &text[at + 1..];
    let len = rest.bytes().take_while(|b| is_name_char(*b)).count();
    // A trailing `.`, `-` or `_` is punctuation ("thanks @alice.").
    let name = rest[..len].trim_end_matches(['.', '-', '_']);
    if name.is_empty() || rest[name.len()..].starts_with('@') {
        return None;
    }
    name.parse::<AccountName>().ok()?;
    Some(Found { start: at, end: at + 1 + name.len(), reference: Reference::Mention(name.to_owned()), closes: false })
}

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

/// The references in plain text (commit messages).
pub fn scan_plain(text: &str) -> Scan {
    let mut scan = Scan::default();
    scan.add_text(text);
    scan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(value: u32) -> IssueNumber {
        IssueNumber::new(value).unwrap()
    }

    fn remote(owner: &str, repo: &str, number: u32) -> RemoteRef {
        RemoteRef { owner: owner.to_owned(), repo: repo.to_owned(), number: n(number) }
    }

    fn found(text: &str) -> Vec<(&str, Reference, bool)> {
        find(text).into_iter().map(|f| (&text[f.start..f.end], f.reference, f.closes)).collect()
    }

    type Case<'a> = (&'a str, Vec<(&'a str, Reference, bool)>);

    #[test]
    fn finds_each_form_with_its_range() {
        use Reference::{Local, Mention, Remote};
        let cases: Vec<Case<'_>> = vec![
            ("#12", vec![("#12", Local(n(12)), false)]),
            ("see #3, and (#4).", vec![("#3", Local(n(3)), false), ("#4", Local(n(4)), false)]),
            ("acme/web#7 here", vec![("acme/web#7", Remote(remote("acme", "web", 7)), false)]),
            ("a.b/c-d.e#7", vec![("a.b/c-d.e#7", Remote(remote("a.b", "c-d.e", 7)), false)]),
            ("cc @alice, @Bob.", vec![("@alice", Mention("alice".into()), false), ("@Bob", Mention("Bob".into()), false)]),
            ("@a-b-", vec![("@a-b", Mention("a-b".into()), false)]),
            ("Fixes #1", vec![("#1", Local(n(1)), true)]),
            ("fixes: #2 and closes #3", vec![("#2", Local(n(2)), true), ("#3", Local(n(3)), true)]),
            ("RESOLVED:#4", vec![("#4", Local(n(4)), true)]),
            ("prefixes #5", vec![("#5", Local(n(5)), false)]),
            ("fix  #6", vec![("#6", Local(n(6)), true)]),
            ("fixes acme/web#8", vec![("acme/web#8", Remote(remote("acme", "web", 8)), false)]),
            ("#4294967295", vec![("#4294967295", Local(n(u32::MAX)), false)]),
        ];
        for (text, expected) in cases {
            assert_eq!(found(text), expected, "{text:?}");
        }
    }

    #[test]
    fn ignores_what_is_not_a_reference() {
        for text in [
            "#0",
            "#012",
            "#4294967296",
            "#99999999999",
            "abc#12",
            "#12abc",
            "#12_",
            "a@b.com",
            "alice@example.com",
            "@",
            "@-x",
            "#",
            "##12",
            "&#12;",
            "x/#1",
            "/repo#1",
            "fixes#1 is not closing",
            "@alice@example.com",
            "a/b/c#1",
        ] {
            assert_eq!(found(text), [], "{text:?}");
        }
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

    #[test]
    fn scans_deduplicate_and_cap_mentions() {
        let text = (0..80).map(|i| format!("@user{i} @USER{i} #1")).collect::<Vec<_>>().join(" ");
        let scan = scan_plain(&text);
        assert_eq!(scan.mentions.len(), MAX_MENTIONS);
        assert_eq!(scan.mentions[0], "user0");
        assert_eq!(scan.local, [n(1)]);
        let mut merged = scan_plain("#2 fixes #1");
        merged.extend(scan_plain("#1 @x"));
        assert_eq!((merged.local, merged.closes, merged.mentions), (vec![n(2), n(1)], vec![n(1)], vec!["x".to_owned()]));
    }

    #[test]
    fn handles_unicode_around_references() {
        assert_eq!(found("é#1 ü @ä"), vec![("#1", Reference::Local(n(1)), false)]);
        assert_eq!(found("→#2"), vec![("#2", Reference::Local(n(2)), false)]);
    }
}
