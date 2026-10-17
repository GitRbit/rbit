//! Every email rbit sends.
//!
//! Each message is a plain struct holding exactly what its templates show. Two Askama
//! templates render it: `templates/<name>.txt` (the plain-text part, not escaped) and
//! `templates/<name>.html` (the HTML part, auto-escaped, built on `layout.html`). The HTML is
//! deliberately plain and monochrome, with inline styles only, because mail clients ignore
//! stylesheets.

use askama::Template;
use rbit_core::Email;

use crate::MailError;

/// A rendered message, ready for any transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: Email,
    pub subject: String,
    /// The `text/plain` part.
    pub text: String,
    /// The `text/html` part.
    pub html: String,
    /// Threading and list headers; empty for one-off messages.
    pub headers: MailHeaders,
}

/// Optional headers of a message. Message ids are written with their angle brackets
/// (`<owner/repo/issues/1@host>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MailHeaders {
    /// `Message-ID`; the relay assigns one when `None`.
    pub message_id: Option<String>,
    /// `In-Reply-To` and `References`: the message this one continues.
    pub in_reply_to: Option<String>,
    /// `List-Unsubscribe` (RFC 2369) with `List-Unsubscribe-Post: List-Unsubscribe=One-Click`
    /// (RFC 8058): an HTTPS link that unsubscribes with a single `POST`.
    pub one_click_unsubscribe: Option<String>,
}

impl MailHeaders {
    /// Each header as `(name, value)`, in the order they are written.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(id) = &self.message_id {
            pairs.push(("Message-ID", id.clone()));
        }
        if let Some(parent) = &self.in_reply_to {
            pairs.push(("In-Reply-To", parent.clone()));
            pairs.push(("References", parent.clone()));
        }
        if let Some(link) = &self.one_click_unsubscribe {
            pairs.push(("List-Unsubscribe", format!("<{link}>")));
            pairs.push(("List-Unsubscribe-Post", "List-Unsubscribe=One-Click".to_owned()));
        }
        pairs
    }
}

impl Mail {
    /// Renders `message` for `to`.
    pub fn render(to: Email, message: &impl MailMessage) -> Result<Self, MailError> {
        Ok(Self {
            to,
            subject: message.subject(),
            text: message.render_text()?,
            html: message.render_html()?,
            headers: MailHeaders::default(),
        })
    }

    /// The same message with `headers`.
    #[must_use]
    pub fn with_headers(mut self, headers: MailHeaders) -> Self {
        self.headers = headers;
        self
    }

    /// Every `http(s)://` link in the plain-text part, in order.
    pub fn links(&self) -> Vec<&str> {
        self.text
            .split_whitespace()
            .map(|word| word.trim_start_matches(['(', '<']))
            .filter(|word| word.starts_with("http://") || word.starts_with("https://"))
            .map(|word| word.trim_end_matches(['.', ',', ')', '>']))
            .collect()
    }
}

/// A message that can be rendered into a [`Mail`].
pub trait MailMessage {
    fn subject(&self) -> String;
    fn render_text(&self) -> Result<String, MailError>;
    fn render_html(&self) -> Result<String, MailError>;
}

/// Wires a message struct to its two templates.
macro_rules! templates {
    ($message:ident, text = $text:ident, html = $html:ident) => {
        impl MailMessage for $message {
            fn subject(&self) -> String {
                $message::subject(self)
            }

            fn render_text(&self) -> Result<String, MailError> {
                Ok($text { m: self }.render()?)
            }

            fn render_html(&self) -> Result<String, MailError> {
                Ok($html { m: self }.render()?)
            }
        }
    };
}

// ------------------------------------------------------------------ password reset

/// Sent from `/password/forgot` (or by an administrator): a link to choose a new password.
#[derive(Debug, Clone)]
pub struct PasswordReset {
    pub site_name: String,
    pub base_url: String,
    pub username: String,
    pub link: String,
    /// e.g. `1 hour`.
    pub valid_for: String,
}

impl PasswordReset {
    fn subject(&self) -> String {
        format!("Reset your {} password", self.site_name)
    }
}

#[derive(Template)]
#[template(path = "password_reset.txt")]
struct PasswordResetText<'a> {
    m: &'a PasswordReset,
}

#[derive(Template)]
#[template(path = "password_reset.html")]
struct PasswordResetHtml<'a> {
    m: &'a PasswordReset,
}

templates!(PasswordReset, text = PasswordResetText, html = PasswordResetHtml);

// ------------------------------------------------------------------ verification

/// Why a verification link is being sent; only the opening sentence differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyReason {
    /// The account was just created.
    SignUp,
    /// The address replaced an earlier one.
    Changed,
    /// The user asked for another link.
    Requested,
}

/// Sent after sign-up, after an address change or on request: a link that confirms the address.
#[derive(Debug, Clone)]
pub struct VerifyEmail {
    pub site_name: String,
    pub base_url: String,
    pub username: String,
    pub link: String,
    /// e.g. `2 days`.
    pub valid_for: String,
    pub reason: VerifyReason,
}

impl VerifyEmail {
    fn subject(&self) -> String {
        format!("Confirm your email address for {}", self.site_name)
    }

    /// The opening sentence, which leads into the link.
    pub fn introduction(&self) -> String {
        match self.reason {
            VerifyReason::SignUp => format!("Welcome to {}! Confirm that this email address belongs to you:", self.site_name),
            VerifyReason::Changed => {
                format!("You set a new email address for your {} account. Confirm that it belongs to you:", self.site_name)
            }
            VerifyReason::Requested => format!("Confirm that this email address belongs to your {} account:", self.site_name),
        }
    }
}

#[derive(Template)]
#[template(path = "verify_email.txt")]
struct VerifyEmailText<'a> {
    m: &'a VerifyEmail,
}

#[derive(Template)]
#[template(path = "verify_email.html")]
struct VerifyEmailHtml<'a> {
    m: &'a VerifyEmail,
}

templates!(VerifyEmail, text = VerifyEmailText, html = VerifyEmailHtml);

// ------------------------------------------------------------------ email changed

/// Sent to the *previous* address after a change, so a hijacked account does not go unnoticed.
#[derive(Debug, Clone)]
pub struct EmailChanged {
    pub site_name: String,
    pub base_url: String,
    pub username: String,
    /// The new address, partly masked (see [`mask_address`]).
    pub new_address: String,
}

impl EmailChanged {
    fn subject(&self) -> String {
        format!("Your {} email address was changed", self.site_name)
    }
}

#[derive(Template)]
#[template(path = "email_changed.txt")]
struct EmailChangedText<'a> {
    m: &'a EmailChanged,
}

#[derive(Template)]
#[template(path = "email_changed.html")]
struct EmailChangedHtml<'a> {
    m: &'a EmailChanged,
}

templates!(EmailChanged, text = EmailChangedText, html = EmailChangedHtml);

// ------------------------------------------------------------------ thread notification

/// Milestone 3 workstream N: activity on an issue or pull request the recipient follows
/// (a mention, an assignment, a review request, a review, a comment, a state change).
/// Short on purpose: what happened, an excerpt, a link to the thread, why the recipient
/// got it, and links to the notification settings and to stop these emails.
#[derive(Debug, Clone)]
pub struct ThreadNotification {
    pub site_name: String,
    pub base_url: String,
    /// `owner/repo`.
    pub repository: String,
    pub number: u64,
    pub title: String,
    /// One sentence, e.g. `bob commented on issue #3.`
    pub headline: String,
    /// The comment, description or review text (Markdown source, already shortened), or empty.
    pub excerpt: String,
    /// The thread, anchored at the activity.
    pub link: String,
    /// Whether this is a pull request (the button says so).
    pub is_pull: bool,
    /// Whether the message continues the thread (the subject starts with `Re:`).
    pub is_reply: bool,
    /// e.g. `You are receiving this because you were mentioned.`
    pub reason: String,
    pub settings_link: String,
    /// The page that turns notification emails off (the link in `List-Unsubscribe`).
    pub unsubscribe_link: String,
}

impl ThreadNotification {
    fn subject(&self) -> String {
        let prefix = if self.is_reply { "Re: " } else { "" };
        format!("{prefix}[{}] {} (#{})", self.repository, self.title, self.number)
    }

    /// The button label.
    pub fn action_label(&self) -> &'static str {
        if self.is_pull { "View pull request" } else { "View issue" }
    }
}

#[derive(Template)]
#[template(path = "thread_notification.txt")]
struct ThreadNotificationText<'a> {
    m: &'a ThreadNotification,
}

#[derive(Template)]
#[template(path = "thread_notification.html")]
struct ThreadNotificationHtml<'a> {
    m: &'a ThreadNotification,
}

templates!(ThreadNotification, text = ThreadNotificationText, html = ThreadNotificationHtml);

// ------------------------------------------------------------------ helpers

/// `alice@example.com` → `a••••@example.com`: enough for the owner to recognise, without
/// handing the full new address to whoever reads the old mailbox.
pub fn mask_address(address: &str) -> String {
    match address.split_once('@') {
        Some((local, domain)) => {
            let first = local.chars().next().map(String::from).unwrap_or_default();
            format!("{first}••••@{domain}")
        }
        None => "••••".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            to: to(),
            subject: String::new(),
            text: "Go to https://a.example/x. Or (http://b.example/y) now".to_owned(),
            html: String::new(),
            headers: MailHeaders::default(),
        };
        assert_eq!(mail.links(), ["https://a.example/x", "http://b.example/y"]);
    }
}
