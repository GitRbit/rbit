//! Reference lexer boundaries remain independent from Markdown/runtime features.
use rbit_collaboration::{
    references::{MAX_MENTIONS, Reference, RemoteRef, Scan, find, scan_plain},
    threads::IssueNumber,
};
#[test]
fn ranges_keywords_remote_names_and_unicode_keep_their_original_contracts() {
    let text = "é#1 **Fixes** #2 alice/repo#3 @Bob.";
    let found = find(text);
    assert_eq!(found.iter().map(|found| &text[found.start..found.end]).collect::<Vec<_>>(), ["#1", "#2", "alice/repo#3", "@Bob"]);
    assert!(matches!(&found[2].reference,Reference::Remote(RemoteRef {owner,repo,..}) if owner=="alice"&&repo=="repo"));
    for bad in ["#0", "#01", "#4294967296", "abc#1", "x/y/z#1", "alice@example.com"] {
        assert_eq!(find(bad), []);
    }
    let scan = scan_plain("Fixes: #2 #1 #2 @Bob @bob alice/repo#3");
    assert_eq!(scan.local, [IssueNumber::new(2).unwrap(), IssueNumber::new(1).unwrap()]);
    assert_eq!(scan.closes, [IssueNumber::new(2).unwrap()]);
    assert_eq!(scan.mentions, ["bob"]);
}
#[test]
fn scan_extension_caps_mentions_and_keeps_first_appearance_order() {
    let mut scan = Scan::default();
    for id in 0..80 {
        scan.add_text(&format!("@user{id} "));
    }
    assert_eq!(scan.mentions.len(), MAX_MENTIONS);
    scan.extend(scan_plain("#2 #1 #2"));
    assert_eq!(scan.local, [IssueNumber::new(2).unwrap(), IssueNumber::new(1).unwrap()]);
}
