//! Plain-text reference lexer, preserving byte ranges and closing keyword rules.
use super::{CLOSING_KEYWORDS, Found, Reference, RemoteRef, Scan};
use crate::threads::IssueNumber;
use rbit_identity_models::AccountName;
use rbit_repositories::RepoName;

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

/// The references in plain text (commit messages).
pub fn scan_plain(text: &str) -> Scan {
    let mut scan = Scan::default();
    scan.add_text(text);
    scan
}
