//! Rendered mail envelope and the transport-independent rendering contract.

use super::MailHeaders;
use crate::MailError;
use rbit_core::Email;

/// A rendered message, ready for any transport.
#[derive(Clone, PartialEq, Eq)]
pub struct Mail {
    /// Optional live instance name for the sender; the configured address is retained.
    pub from_name: Option<String>,
    pub to: Email,
    pub subject: String,
    /// The `text/plain` part.
    pub text: String,
    /// The `text/html` part.
    pub html: String,
    /// Threading and list headers; empty for one-off messages.
    pub headers: MailHeaders,
}

impl Mail {
    /// Renders `message` for `to`.
    pub fn render(to: Email, message: &impl MailMessage) -> Result<Self, MailError> {
        Ok(Self {
            from_name: None,
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

impl std::fmt::Debug for Mail {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Mail").finish_non_exhaustive()
    }
}
