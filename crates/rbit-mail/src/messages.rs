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
}

impl Mail {
    /// Renders `message` for `to`.
    pub fn render(to: Email, message: &impl MailMessage) -> Result<Self, MailError> {
        Ok(Self { to, subject: message.subject(), text: message.render_text()?, html: message.render_html()? })
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
        };
        assert_eq!(mail.links(), ["https://a.example/x", "http://b.example/y"]);
    }
}
