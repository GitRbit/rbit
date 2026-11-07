//! Existing rendered-message, escaping, header and link acceptance fixtures.

use super::*;
use rbit_core::Email;

const SITE: &str = "Forge <&>";

fn to() -> Email {
    "alice@example.com".parse().unwrap()
}

fn assert_well_formed(mail: &Mail) {
    assert!(mail.html.starts_with("<!doctype html>"), "HTML part has a doctype");
    assert!(mail.html.contains("Forge &#60;&#38;&#62;") || mail.html.contains("Forge &lt;&amp;&gt;"), "site name is escaped");
    assert!(!mail.html.contains("<script"), "no scripts");
    assert!(mail.text.contains("Forge <&>"), "plain text is not escaped");
    assert!(mail.text.contains("https://git.example.com"), "footer names the instance");
}

#[test]
fn renders_password_reset() {
    let message = PasswordReset {
        site_name: SITE.to_owned(),
        base_url: "https://git.example.com".to_owned(),
        username: "alice".to_owned(),
        link: "https://git.example.com/password/reset/abc_DEF-123".to_owned(),
        valid_for: "1 hour".to_owned(),
    };
    let mail = Mail::render(to(), &message).unwrap();
    assert_eq!(mail.subject, "Reset your Forge <&> password");
    assert_eq!(mail.links().first().copied(), Some("https://git.example.com/password/reset/abc_DEF-123"));
    assert!(mail.text.contains("Hi alice,"));
    assert!(mail.text.contains("1 hour"));
    assert!(mail.html.contains(r#"href="https://git.example.com/password/reset/abc_DEF-123""#));
    assert_well_formed(&mail);
}

#[test]
fn renders_verification_for_new_and_changed_addresses() {
    let mut message = VerifyEmail {
        site_name: SITE.to_owned(),
        base_url: "https://git.example.com".to_owned(),
        username: "alice".to_owned(),
        link: "https://git.example.com/email/verify/tok".to_owned(),
        valid_for: "2 days".to_owned(),
        reason: VerifyReason::SignUp,
    };
    let mail = Mail::render(to(), &message).unwrap();
    assert_eq!(mail.subject, "Confirm your email address for Forge <&>");
    assert!(mail.text.contains("Welcome to Forge <&>!"));
    assert!(mail.text.contains("Hi alice,\n\nWelcome to"), "no stray blank lines: {:?}", mail.text);
    assert!(mail.links().contains(&"https://git.example.com/email/verify/tok"));
    assert!(mail.html.contains(r#"href="https://git.example.com/email/verify/tok""#));
    assert_well_formed(&mail);

    message.reason = VerifyReason::Changed;
    let mail = Mail::render(to(), &message).unwrap();
    assert!(!mail.text.contains("Welcome to"));
    assert!(mail.text.contains("new email address"));
    assert!(mail.html.contains("new email address"));

    message.reason = VerifyReason::Requested;
    let mail = Mail::render(to(), &message).unwrap();
    assert!(mail.text.contains("belongs to your Forge <&> account"));
}

#[test]
fn renders_email_changed_notice() {
    let message = EmailChanged {
        site_name: SITE.to_owned(),
        base_url: "https://git.example.com".to_owned(),
        username: "alice".to_owned(),
        new_address: mask_address("new@example.org"),
    };
    let mail = Mail::render(to(), &message).unwrap();
    assert_eq!(mail.subject, "Your Forge <&> email address was changed");
    assert!(mail.text.contains("n••••@example.org"));
    assert!(mail.text.contains("administrator"));
    assert!(mail.html.contains("n••••@example.org"));
    assert_well_formed(&mail);
}

fn thread_notification() -> ThreadNotification {
    ThreadNotification {
        site_name: SITE.to_owned(),
        base_url: "https://git.example.com".to_owned(),
        repository: "alice/app".to_owned(),
        number: 3,
        title: "Crash on <start>".to_owned(),
        headline: "bob commented on issue #3.".to_owned(),
        excerpt: "Same here, @carol <b>".to_owned(),
        link: "https://git.example.com/alice/app/issues/3#issuecomment-9".to_owned(),
        is_pull: false,
        is_reply: true,
        reason: "You are receiving this because you were mentioned.".to_owned(),
        settings_link: "https://git.example.com/settings/notifications".to_owned(),
        unsubscribe_link: "https://git.example.com/email/unsubscribe/2.abc".to_owned(),
    }
}

#[test]
fn renders_thread_notifications() {
    let mail = Mail::render(to(), &thread_notification()).unwrap();
    assert_eq!(mail.subject, "Re: [alice/app] Crash on <start> (#3)");
    assert_eq!(
        mail.links(),
        [
            "https://git.example.com/alice/app/issues/3#issuecomment-9",
            "https://git.example.com/settings/notifications",
            "https://git.example.com/email/unsubscribe/2.abc",
            "https://git.example.com",
        ]
    );
    assert!(mail.text.starts_with("bob commented on issue #3.\n\nSame here, @carol <b>\n\nView it on Forge <&>:"), "{:?}", mail.text);
    assert!(mail.text.contains("You are receiving this because you were mentioned."), "says why");
    assert!(mail.html.contains(r#"href="https://git.example.com/alice/app/issues/3#issuecomment-9""#));
    assert!(mail.html.contains(r#"href="https://git.example.com/email/unsubscribe/2.abc""#));
    assert!(mail.html.contains(r#"href="https://git.example.com/settings/notifications""#));
    assert!(mail.html.contains("View issue"));
    assert!(!mail.html.contains("<b>"), "the excerpt is escaped");
    assert!(!mail.html.contains("<start>"), "the title is escaped");
    assert!(!mail.html.contains("because of an account on"), "the generic footer is replaced");
    assert_well_formed(&mail);
    assert_eq!(mail.headers, MailHeaders::default(), "the caller adds the threading headers");

    let opening = ThreadNotification { is_reply: false, is_pull: true, excerpt: String::new(), ..thread_notification() };
    let mail = Mail::render(to(), &opening).unwrap();
    assert_eq!(mail.subject, "[alice/app] Crash on <start> (#3)");
    assert!(mail.text.starts_with("bob commented on issue #3.\n\nView it on"), "no empty excerpt: {:?}", mail.text);
    assert!(mail.html.contains("View pull request"));
}

fn digest() -> DigestNotification {
    let entry = |headline: &str, excerpt: &str, link: &str| DigestEntry {
        headline: headline.to_owned(),
        excerpt: excerpt.to_owned(),
        link: link.to_owned(),
    };
    DigestNotification {
        site_name: SITE.to_owned(),
        base_url: "https://git.example.com".to_owned(),
        weekly: false,
        repositories: vec![
            DigestRepository {
                name: "alice/app".to_owned(),
                link: "https://git.example.com/alice/app".to_owned(),
                threads: vec![DigestThread {
                    number: 3,
                    title: "Crash on <start>".to_owned(),
                    is_pull: false,
                    link: "https://git.example.com/alice/app/issues/3".to_owned(),
                    entries: vec![
                        entry("bob commented on issue #3.", "Same here <b>", "https://git.example.com/alice/app/issues/3#issuecomment-9"),
                        entry("carol closed issue #3 as completed.", "", "https://git.example.com/alice/app/issues/3"),
                    ],
                }],
            },
            DigestRepository {
                name: "bob/lib".to_owned(),
                link: "https://git.example.com/bob/lib".to_owned(),
                threads: vec![DigestThread {
                    number: 7,
                    title: "Faster parser".to_owned(),
                    is_pull: true,
                    link: "https://git.example.com/bob/lib/pulls/7".to_owned(),
                    entries: vec![entry("bob requested your review on pull request #7.", "", "https://git.example.com/bob/lib/pulls/7")],
                }],
            },
        ],
        settings_link: "https://git.example.com/settings/notifications".to_owned(),
        unsubscribe_link: "https://git.example.com/email/unsubscribe/2.abc".to_owned(),
    }
}

#[test]
fn renders_digests_grouped_by_repository() {
    let mail = Mail::render(to(), &digest()).unwrap();
    assert_eq!(mail.subject, "Your daily digest: 3 notifications in 2 repositories");
    let expected = [
        "Your daily digest: 3 notifications in 2 repositories.",
        "",
        "alice/app",
        "",
        "Issue #3: Crash on <start>",
        "- bob commented on issue #3.",
        "  Same here <b>",
        "  https://git.example.com/alice/app/issues/3#issuecomment-9",
        "- carol closed issue #3 as completed.",
        "  https://git.example.com/alice/app/issues/3",
        "",
        "bob/lib",
        "",
        "Pull request #7: Faster parser",
        "- bob requested your review on pull request #7.",
        "  https://git.example.com/bob/lib/pulls/7",
        "",
        "You are receiving this digest because you chose daily notification emails.",
        "Notification settings: https://git.example.com/settings/notifications",
        "Stop notification emails: https://git.example.com/email/unsubscribe/2.abc",
        "",
        "--",
        "Forge <&>",
        "https://git.example.com",
    ];
    assert_eq!(mail.text.trim_end().lines().collect::<Vec<_>>(), expected, "{}", mail.text);
    assert!(mail.html.contains(r#"href="https://git.example.com/alice/app/issues/3#issuecomment-9""#));
    assert!(mail.html.contains(r#"href="https://git.example.com/alice/app""#), "repositories link to themselves");
    assert!(mail.html.contains(r#"href="https://git.example.com/email/unsubscribe/2.abc""#));
    assert!(mail.html.contains("Pull request #7"));
    assert!(!mail.html.contains("<b>") && !mail.html.contains("<start>"), "text is escaped");
    assert!(mail.html.find("alice/app").unwrap() < mail.html.find("bob/lib").unwrap(), "in the given order");
    assert_well_formed(&mail);

    let weekly = DigestNotification { weekly: true, repositories: digest().repositories[1..].to_vec(), ..digest() };
    let mail = Mail::render(to(), &weekly).unwrap();
    assert_eq!(mail.subject, "Your weekly digest: 1 notification in 1 repository");
    assert!(mail.text.contains("because you chose weekly notification emails"));
}

#[test]
fn lists_headers_in_order() {
    assert_eq!(MailHeaders::default().pairs(), []);
    let headers = MailHeaders {
        message_id: Some("<a@h>".to_owned()),
        in_reply_to: Some("<root@h>".to_owned()),
        one_click_unsubscribe: Some("https://h/u".to_owned()),
    };
    let names: Vec<&str> = headers.pairs().iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["Message-ID", "In-Reply-To", "References", "List-Unsubscribe", "List-Unsubscribe-Post"]);
    assert_eq!(headers.pairs()[3].1, "<https://h/u>");
}

#[test]
fn masks_addresses() {
    assert_eq!(mask_address("alice@example.com"), "a••••@example.com");
    assert_eq!(mask_address("@example.com"), "••••@example.com");
    assert_eq!(mask_address("garbage"), "••••");
}

#[test]
fn finds_links_in_text() {
    let mail = Mail {
        from_name: None,
        to: to(),
        subject: String::new(),
        text: "Go to https://a.example/x. Or (http://b.example/y) now".to_owned(),
        html: String::new(),
        headers: MailHeaders::default(),
    };
    assert_eq!(mail.links(), ["https://a.example/x", "http://b.example/y"]);
}
