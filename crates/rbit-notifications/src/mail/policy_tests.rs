use super::*;

#[test]
fn preferences_decide_by_reason() {
    for reason in NotificationReason::ALL {
        assert!(!wants(MailPreference::Off, reason));
        assert!(wants(MailPreference::All, reason));
        assert_eq!(wants(MailPreference::Participating, reason), reason != NotificationReason::Manual, "{reason:?}");
    }
}

#[test]
fn message_ids_use_the_bare_host() {
    assert_eq!(message_id_host("https://git.example.com"), "git.example.com");
    assert_eq!(message_id_host("http://127.0.0.1:3000"), "127.0.0.1");
    assert_eq!(message_id_host("https://Git.Example.com:8443/prefix"), "git.example.com");
    assert_eq!(message_id_host("http://[::1]:3000"), "[::1]");
    assert_eq!(message_id_host("https://user@host.example"), "host.example");
    assert_eq!(message_id_host(""), "localhost");
}

#[test]
fn excerpts_are_short() {
    assert_eq!(excerpt_of("  hello \n"), "hello");
    let long = "é".repeat(MAX_EXCERPT_CHARS + 5);
    let excerpt = excerpt_of(&long);
    assert_eq!(excerpt.chars().count(), MAX_EXCERPT_CHARS + 1);
    assert!(excerpt.ends_with('…'));
    assert_eq!(one_line_excerpt("  Same\n\nhere,\tplease "), "Same here, please");
    let excerpt = one_line_excerpt(&"word ".repeat(100));
    assert_eq!(excerpt.chars().count(), MAX_DIGEST_EXCERPT_CHARS);
    assert!(excerpt.ends_with("word…"), "cut at a word, trailing space dropped: {excerpt:?}");
    assert_eq!(capitalised("issue #3 was closed."), "Issue #3 was closed.");
    assert_eq!(capitalised(""), "");
}

#[test]
fn threading_links_and_hours_keep_their_shapes() {
    assert_eq!(thread_path("a/b", IssueKind::Pull, 7), "a/b/pulls/7");
    assert_eq!(thread_message_ids("a/b/issues/1", "h", "opened", true), ("<a/b/issues/1@h>".to_owned(), None));
    let reply = thread_message_ids("a/b/issues/1", "h", "comment-9", false);
    assert_eq!(reply, ("<a/b/issues/1/comment-9@h>".to_owned(), Some("<a/b/issues/1@h>".to_owned())));
    assert_eq!(digest_message_id(4, 86_400, "h"), "<digest/4/86400@h>");
    assert_eq!(thread_link("https://x", "a/b/issues/1", Some("issuecomment-2")), "https://x/a/b/issues/1#issuecomment-2");
    assert_eq!(thread_link("https://x", "a/b/issues/1", None), "https://x/a/b/issues/1");
    assert!(valid_digest_hour(None) && valid_digest_hour(Some(23)) && !valid_digest_hour(Some(24)));
}
