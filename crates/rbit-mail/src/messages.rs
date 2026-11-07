//! Every email rbit sends.
//!
//! Each message is a plain struct holding exactly what its templates show. Two Askama
//! templates render it: `templates/<name>.txt` (the plain-text part, not escaped) and
//! `templates/<name>.html` (the HTML part, auto-escaped, built on `layout.html`). The HTML is
//! deliberately plain and monochrome, with inline styles only, because mail clients ignore
//! stylesheets.

pub use self::authentication::{EmailChanged, PasswordReset, VerifyEmail, VerifyReason};
pub use self::digest::{DigestEntry, DigestNotification, DigestRepository, DigestThread};
pub use self::envelope::{Mail, MailMessage};
pub use self::headers::MailHeaders;
pub use self::thread::ThreadNotification;

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

mod authentication;
mod digest;
mod envelope;
mod headers;
mod thread;

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
mod tests;
