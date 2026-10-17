//! Property tests for the web layer's text handling: rendered Markdown never contains
//! script, event handlers or script URLs whatever the input, resolved relative links
//! never leave the repository, and path encoding round-trips while keeping every
//! segment URL-safe.

use percent_encoding::percent_decode_str;
use proptest::prelude::*;
use rbit_core::{IssueKind, IssueNumber, IssueState};
use rbit_services::references::{ReferenceSet, ReferencedIssue};
use rbit_web::{
    render::{
        references::{RefContext, render_issue_markdown},
        render_markdown,
    },
    urls::{DocumentBase, RepoUrls, encode_path},
};

/// One parsed start tag: its lowercase name and `(name, value)` attributes.
#[derive(Debug)]
struct Tag {
    name: String,
    attributes: Vec<(String, String)>,
}

/// A small, quote-aware HTML start-tag scanner. The sanitiser escapes `<` in text, so
/// every `<` in its output opens a tag (or a comment, which it never emits).
fn start_tags(html: &str) -> Vec<Tag> {
    let bytes = html.as_bytes();
    let mut tags = Vec::new();
    let mut i = 0;
    while let Some(offset) = html[i..].find('<') {
        i += offset + 1;
        if html[i..].starts_with('/') || html[i..].starts_with('!') {
            continue;
        }
        let name_len = html[i..].find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').unwrap_or(html.len() - i);
        let name = html[i..i + name_len].to_ascii_lowercase();
        i += name_len;
        let mut attributes = Vec::new();
        loop {
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b'>' {
                break;
            }
            let attr_len = html[i..].find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '>' || c == '/').unwrap_or(html.len() - i);
            let attr = html[i..i + attr_len].to_ascii_lowercase();
            i += attr_len;
            let value = if i < bytes.len() && bytes[i] == b'=' {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i] as char;
                    let end = html[i + 1..].find(quote).map_or(html.len(), |e| i + 1 + e);
                    let quoted = html[i + 1..end].to_owned();
                    i = (end + 1).min(html.len());
                    quoted
                } else {
                    let end = html[i..].find(|c: char| c.is_ascii_whitespace() || c == '>').map_or(html.len(), |e| i + e);
                    let bare = html[i..end].to_owned();
                    i = end;
                    bare
                }
            } else {
                String::new()
            };
            if attr.is_empty() {
                i += 1;
            } else {
                attributes.push((attr, value));
            }
        }
        tags.push(Tag { name, attributes });
    }
    tags
}

/// Decodes the entities ammonia emits in attribute values, so `javascript&#58;` is caught.
fn decode_entities(value: &str) -> String {
    value.replace("&amp;", "&").replace("&quot;", "\"").replace("&#58;", ":").replace("&colon;", ":").replace("&#x3a;", ":")
}

/// Markdown made of hostile and ordinary fragments.
fn hostile_markdown() -> impl Strategy<Value = String> {
    let fragment = prop_oneof![
        Just("<script>alert(1)</script>".to_owned()),
        Just("<SCRIPT SRC=//evil.example/x.js></SCRIPT>".to_owned()),
        Just("<scr<script>ipt>alert(1)</script>".to_owned()),
        Just("<img src=x onerror=alert(1)>".to_owned()),
        Just("<img src=\"x\" ONERROR=\"alert(1)\">".to_owned()),
        Just("<svg onload=alert(1)><circle/></svg>".to_owned()),
        Just("<a href=\"javascript:alert(1)\">x</a>".to_owned()),
        Just("<a href=\"JaVaScRiPt:alert(1)\">x</a>".to_owned()),
        Just("<a href=\"java\tscript:alert(1)\">x</a>".to_owned()),
        Just("[link](javascript:alert(1))".to_owned()),
        Just("![img](javascript:alert(1))".to_owned()),
        Just("<details open ontoggle=alert(1)><summary>s</summary></details>".to_owned()),
        Just("<input type=checkbox onfocus=alert(1) autofocus>".to_owned()),
        Just("<div style=\"background:url(javascript:alert(1))\" onclick='x'>d</div>".to_owned()),
        Just("<iframe src=\"https://evil.example\"></iframe>".to_owned()),
        Just("<object data=x></object><embed src=x>".to_owned()),
        Just("<math><mtext><table><mglyph><style><img src=x onerror=alert(1)>".to_owned()),
        Just("<p align=\"left\" onmouseover=\"x\">p</p>".to_owned()),
        Just("<source srcset=x onerror=alert(1)>".to_owned()),
        Just("<!-- <script>x</script> -->".to_owned()),
        Just("```html\n<script>alert(1)</script>\n```\n".to_owned()),
        Just("`<script>`".to_owned()),
        Just("# Heading {#id onclick=x}\n".to_owned()),
        Just("| a | <script>b</script> |\n|---|---|\n| c | d |\n".to_owned()),
        Just("- [x] done <img src=x onerror=y>\n".to_owned()),
        Just("[^1]\n\n[^1]: <script>x</script>\n".to_owned()),
        Just("\n\n".to_owned()),
        Just(" ".to_owned()),
        Just("Fixes #1, cc @alice and acme/web#2 ".to_owned()),
        Just("#1\" onmouseover=\"x @alice<img src=x onerror=y> ".to_owned()),
        Just("[#1 @alice](javascript:alert(1)) `#1`".to_owned()),
        "[a-z<>=\"' /:()#@]{0,20}",
        any::<String>(),
    ];
    prop::collection::vec(fragment, 0..12).prop_map(|parts| parts.concat())
}

/// A thread context in which `#1` and `@alice` resolve, so issue Markdown inserts links.
fn issue_context() -> RefContext {
    let mut refs = ReferenceSet::default();
    let one = IssueNumber::new(1).expect("one");
    refs.issues.insert(
        one,
        ReferencedIssue { number: one, kind: IssueKind::Issue, state: IssueState::Open, title: "\"><script>x</script>".to_owned() },
    );
    refs.accounts.insert("alice".to_owned(), "alice".to_owned());
    RefContext {
        base: DocumentBase::new(RepoUrls::new("alice", "proj"), "main", ""),
        owner: "alice".to_owned(),
        repo: "proj".to_owned(),
        refs,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn markdown_never_contains_script_or_event_handlers(source in hostile_markdown()) {
        // With and without link resolution: the attribute filter must not reopen anything.
        let base = DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "docs");
        for html in [render_markdown(&source, None), render_markdown(&source, Some(&base)), render_issue_markdown(&source, &issue_context())] {
            let lower = html.to_ascii_lowercase();
            prop_assert!(!lower.contains("<script"), "script tag in {:?}", html);
            for tag in start_tags(&html) {
                prop_assert!(!matches!(tag.name.as_str(), "script" | "iframe" | "object" | "embed" | "style" | "svg" | "math"), "<{}> in {:?}", tag.name, html);
                for (name, value) in &tag.attributes {
                    prop_assert!(!name.starts_with("on"), "event handler {}= in {:?}", name, html);
                    prop_assert!(name != "style", "inline style in {:?}", html);
                    let value: String = decode_entities(value).chars().filter(|c| !c.is_ascii_whitespace() && !c.is_ascii_control()).collect();
                    prop_assert!(!value.to_ascii_lowercase().starts_with("javascript:"), "script URL in {:?}", html);
                }
            }
        }
    }

    #[test]
    fn resolved_links_never_leave_the_repository(target in "/?([.]{1,2}/|[a-z]{1,3}/|%2e%2e/){0,6}[a-z]{1,4}") {
        let base = DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "docs");
        let html = render_markdown(&format!("[x]({target}) ![y]({target})"), Some(&base));
        for tag in start_tags(&html) {
            for (name, value) in &tag.attributes {
                if name == "href" || name == "src" {
                    prop_assert!(value.starts_with("/alice/proj/"), "{} escapes: {:?}", name, html);
                    prop_assert!(!value.contains("/../") && !value.ends_with("/.."), "unnormalised {:?}", html);
                }
            }
        }
    }

    #[test]
    fn encoded_paths_round_trip(path in prop::collection::vec(any::<String>(), 1..5).prop_map(|segments| segments.join("/"))) {
        let encoded = encode_path(&path);
        prop_assert_eq!(percent_decode_str(&encoded).decode_utf8().expect("UTF-8"), path.as_str());
        // Slashes survive as separators and nothing else in a segment can split or end the path.
        prop_assert_eq!(encoded.matches('/').count(), path.matches('/').count());
        for segment in encoded.split('/') {
            prop_assert!(!segment.contains(['?', '#', ' ', '"', '<', '>', '`', '{', '}']), "unsafe segment {:?}", segment);
            prop_assert!(segment.chars().all(|c| c.is_ascii() && !c.is_ascii_control()), "non-ASCII or control in {:?}", segment);
            let mut rest = segment;
            while let Some(index) = rest.find('%') {
                let escape = &rest[index + 1..];
                prop_assert!(escape.len() >= 2 && escape.as_bytes()[..2].iter().all(u8::is_ascii_hexdigit), "stray % in {:?}", segment);
                rest = &escape[2..];
            }
        }
    }

    #[test]
    fn safe_paths_are_left_alone(path in "[A-Za-z0-9._~-]{1,12}(/[A-Za-z0-9._~-]{1,12}){0,4}") {
        prop_assert_eq!(encode_path(&path), path);
    }
}

#[test]
fn scanner_sees_handlers_hidden_in_quoted_values() {
    let tags = start_tags("<a title=\"x onclick=y\" href='/z'>t</a><img src=x onerror=alert(1)>");
    assert_eq!(tags[0].attributes, [("title".to_owned(), "x onclick=y".to_owned()), ("href".to_owned(), "/z".to_owned())]);
    assert!(tags[1].attributes.iter().any(|(name, _)| name == "onerror"));
}
