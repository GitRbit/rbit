//! Compatibility paths for the collaboration-owned reference lexer and scan contracts.
//! Markdown scanning is `rbit_collaboration::references::scan_markdown`.
pub use rbit_collaboration::references::{CLOSING_KEYWORDS, Found, MAX_MENTIONS, Reference, RemoteRef, Scan, find, scan_plain};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IssueNumber;

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
