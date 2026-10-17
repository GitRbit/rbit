//! Delivery transports behind the [`Mailer`] seam.
//!
//! Callers hold an `Arc<dyn Mailer>` chosen once from configuration by [`from_config`], so
//! swapping SMTP for the log (or for [`MemoryMailer`] in tests) never changes them.

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{
        Mailbox, MultiPart,
        header::{HeaderName, HeaderValue},
    },
    transport::smtp::authentication::Credentials,
};
use rbit_core::config::{MailConfig, MailTransport};
use tokio::sync::watch;

use crate::{Mail, MailError};

/// The future returned by [`Mailer::send`].
pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<(), MailError>> + Send + 'a>>;

/// Delivers rendered messages.
pub trait Mailer: fmt::Debug + Send + Sync {
    /// Delivers one message. Resolves once the transport has accepted it.
    fn send(&self, mail: Mail) -> SendFuture<'_>;

    /// Whether this transport actually delivers anything (`false` only for [`DisabledMailer`]).
    fn is_enabled(&self) -> bool {
        true
    }
}

/// Builds the transport `config` asks for. `site_name` is the fallback `From:` name.
pub fn from_config(config: &MailConfig, site_name: &str) -> Result<Arc<dyn Mailer>, MailError> {
    Ok(match config.transport {
        MailTransport::Disabled => Arc::new(DisabledMailer),
        MailTransport::Log => Arc::new(LogMailer::new(Sender::from_config(config, site_name)?)),
        MailTransport::Smtp => Arc::new(SmtpMailer::new(config, Sender::from_config(config, site_name)?)?),
    })
}

/// The `From:` mailbox of every message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sender(Mailbox);

impl Sender {
    pub fn new(address: &str, name: &str) -> Result<Self, MailError> {
        let address = parse_address(address)?;
        let name = name.trim();
        Ok(Self(Mailbox::new((!name.is_empty()).then(|| name.to_owned()), address)))
    }

    fn from_config(config: &MailConfig, site_name: &str) -> Result<Self, MailError> {
        let name = if config.from_name.trim().is_empty() { site_name } else { &config.from_name };
        Self::new(&config.from_address, name)
    }
}

impl fmt::Display for Sender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

fn parse_address(address: &str) -> Result<Address, MailError> {
    address
        .trim()
        .parse()
        .map_err(|error: lettre::address::AddressError| MailError::Address { address: address.to_owned(), reason: error.to_string() })
}

/// Builds the MIME message: `multipart/alternative` with the text part first, plus the
/// threading and list headers the message carries.
fn build_message(from: &Sender, mail: Mail) -> Result<Message, MailError> {
    let to = Mailbox::new(None, parse_address(mail.to.as_str())?);
    let mut builder = Message::builder().from(from.0.clone()).to(to).subject(mail.subject);
    let headers = mail.headers;
    if let Some(id) = headers.message_id.clone() {
        builder = builder.message_id(Some(id));
    }
    if let Some(parent) = headers.in_reply_to.clone() {
        builder = builder.in_reply_to(parent.clone()).references(parent);
    }
    if let Some(link) = &headers.one_click_unsubscribe {
        builder = builder
            .raw_header(HeaderValue::new(HeaderName::new_from_ascii_str("List-Unsubscribe"), format!("<{link}>")))
            .raw_header(HeaderValue::new(HeaderName::new_from_ascii_str("List-Unsubscribe-Post"), "List-Unsubscribe=One-Click".to_owned()));
    }
    Ok(builder.multipart(MultiPart::alternative_plain_html(mail.text, mail.html))?)
}

// ------------------------------------------------------------------ SMTP

/// Delivers through an SMTP relay (lettre on tokio, TLS by rustls with the Mozilla roots).
pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Sender,
    relay: String,
}

impl SmtpMailer {
    pub fn new(config: &MailConfig, from: Sender) -> Result<Self, MailError> {
        let host = config.smtp_host.trim();
        if host.is_empty() {
            return Err(MailError::Config("smtp_host is required when transport = \"smtp\"".to_owned()));
        }
        let builder = if config.smtp_implicit_tls() {
            AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
        } else if config.smtp_unencrypted() {
            tracing::warn!(host, "SMTP relay configured without encryption");
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?
        };
        let mut builder = builder.port(config.smtp_port()).timeout(Some(Duration::from_secs(config.timeout_seconds.max(1))));
        if let Some((username, password)) = config.smtp_credentials() {
            builder = builder.credentials(Credentials::new(username.to_owned(), password.to_owned()));
        }
        Ok(Self { transport: builder.build(), from, relay: format!("{host}:{}", config.smtp_port()) })
    }
}

impl fmt::Debug for SmtpMailer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpMailer").field("relay", &self.relay).field("from", &self.from.to_string()).finish_non_exhaustive()
    }
}

impl Mailer for SmtpMailer {
    fn send(&self, mail: Mail) -> SendFuture<'_> {
        Box::pin(async move {
            let message = build_message(&self.from, mail)?;
            self.transport.send(message).await?;
            Ok(())
        })
    }
}

// ------------------------------------------------------------------ log

/// Writes each message (subject, recipient and the plain-text part, links included) to the
/// log at `info` level instead of sending it. For development and first-run setups.
#[derive(Debug, Clone)]
pub struct LogMailer {
    from: Sender,
}

impl LogMailer {
    pub fn new(from: Sender) -> Self {
        Self { from }
    }
}

impl Mailer for LogMailer {
    fn send(&self, mail: Mail) -> SendFuture<'_> {
        Box::pin(async move {
            // Building the message validates it exactly as SMTP delivery would.
            let to = mail.to.clone();
            let subject = mail.subject.clone();
            let text = mail.text.clone();
            let mut extra = String::new();
            for (name, value) in mail.headers.pairs() {
                extra.push('\n');
                extra.push_str(name);
                extra.push_str(": ");
                extra.push_str(&value);
            }
            build_message(&self.from, mail)?;
            // Headers first and the body last, so the message reads naturally in the log.
            tracing::info!("email not sent (log transport)\nFrom: {}\nTo: {to}\nSubject: {subject}{extra}\n\n{text}", self.from);
            Ok(())
        })
    }
}

// ------------------------------------------------------------------ disabled

/// Refuses every message with [`MailError::Disabled`].
#[derive(Debug, Clone, Copy, Default)]
pub struct DisabledMailer;

impl Mailer for DisabledMailer {
    fn send(&self, _mail: Mail) -> SendFuture<'_> {
        Box::pin(async { Err(MailError::Disabled) })
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

// ------------------------------------------------------------------ memory

/// Keeps every message in memory. For tests: clones share one outbox.
#[derive(Debug, Clone)]
pub struct MemoryMailer {
    outbox: Arc<watch::Sender<Vec<Mail>>>,
    failing: Arc<Mutex<bool>>,
}

impl Default for MemoryMailer {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryMailer {
    pub fn new() -> Self {
        Self { outbox: Arc::new(watch::Sender::new(Vec::new())), failing: Arc::new(Mutex::new(false)) }
    }

    /// Every message accepted so far, oldest first.
    pub fn sent(&self) -> Vec<Mail> {
        self.outbox.borrow().clone()
    }

    /// Waits until at least `count` messages were accepted, then returns all of them.
    /// Wrap it in a timeout: it waits forever otherwise.
    pub async fn wait_for(&self, count: usize) -> Vec<Mail> {
        let mut receiver = self.outbox.subscribe();
        let sent = receiver.wait_for(|sent| sent.len() >= count).await.map(|sent| sent.clone());
        sent.unwrap_or_default()
    }

    /// Makes later sends fail (to exercise error handling).
    pub fn fail_sends(&self, failing: bool) {
        *self.failing.lock().unwrap_or_else(PoisonError::into_inner) = failing;
    }
}

impl Mailer for MemoryMailer {
    fn send(&self, mail: Mail) -> SendFuture<'_> {
        Box::pin(async move {
            if *self.failing.lock().unwrap_or_else(PoisonError::into_inner) {
                return Err(MailError::Config("memory mailer set to fail".to_owned()));
            }
            self.outbox.send_modify(|sent| sent.push(mail));
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::config::MailConfig;

    use super::*;

    fn mail() -> Mail {
        Mail {
            to: "bob@example.com".parse().unwrap(),
            subject: "Hello".to_owned(),
            text: "Plain https://example.com/x".to_owned(),
            html: "<p>Rich</p>".to_owned(),
            headers: crate::messages::MailHeaders::default(),
        }
    }

    fn config(transport: MailTransport) -> MailConfig {
        MailConfig { transport, from_address: "git@example.com".to_owned(), ..MailConfig::default() }
    }

    #[test]
    fn sender_falls_back_to_the_site_name() {
        let sender = Sender::from_config(&config(MailTransport::Log), "Forge").unwrap();
        assert_eq!(sender.to_string(), "Forge <git@example.com>");
        let named = MailConfig { from_name: "Ops".to_owned(), ..config(MailTransport::Log) };
        assert_eq!(Sender::from_config(&named, "Forge").unwrap().to_string(), "Ops <git@example.com>");
        assert!(matches!(Sender::new("not an address", ""), Err(MailError::Address { .. })));
    }

    #[test]
    fn builds_multipart_messages() {
        let sender = Sender::new("git@example.com", "Forge").unwrap();
        let formatted = String::from_utf8(build_message(&sender, mail()).unwrap().formatted()).unwrap();
        assert!(formatted.contains("multipart/alternative"));
        assert!(formatted.contains("text/plain"));
        assert!(formatted.contains("text/html"));
        assert!(formatted.contains("To: bob@example.com"));
        assert!(formatted.contains("Subject: Hello"));
        assert!(!formatted.contains("List-Unsubscribe"), "one-off messages carry no list headers");
    }

    #[test]
    fn writes_threading_and_unsubscribe_headers() {
        let sender = Sender::new("git@example.com", "Forge").unwrap();
        let headers = crate::messages::MailHeaders {
            message_id: Some("<alice/app/issues/3/issuecomment-9@git.example.com>".to_owned()),
            in_reply_to: Some("<alice/app/issues/3@git.example.com>".to_owned()),
            one_click_unsubscribe: Some("https://git.example.com/email/unsubscribe/1.abc".to_owned()),
        };
        let formatted = String::from_utf8(build_message(&sender, mail().with_headers(headers)).unwrap().formatted()).unwrap();
        assert!(formatted.contains("Message-ID: <alice/app/issues/3/issuecomment-9@git.example.com>"), "{formatted}");
        assert!(formatted.contains("In-Reply-To: <alice/app/issues/3@git.example.com>"));
        assert!(formatted.contains("References: <alice/app/issues/3@git.example.com>"));
        assert!(formatted.contains("List-Unsubscribe: <https://git.example.com/email/unsubscribe/1.abc>"));
        assert!(formatted.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click"));
        assert!(!formatted.lines().any(|line| line.starts_with("Reply-To:")), "no incoming mail: replies are not invited");
    }

    #[test]
    fn chooses_the_configured_transport() {
        assert!(!from_config(&config(MailTransport::Disabled), "Forge").unwrap().is_enabled());
        assert!(from_config(&config(MailTransport::Log), "Forge").unwrap().is_enabled());

        let missing_host = from_config(&config(MailTransport::Smtp), "Forge");
        assert!(matches!(missing_host, Err(MailError::Config(_))));

        let smtp = MailConfig { smtp_host: "smtp.example.com".to_owned(), ..config(MailTransport::Smtp) };
        let mailer = from_config(&smtp, "Forge").unwrap();
        assert!(mailer.is_enabled());
        assert!(format!("{mailer:?}").contains("smtp.example.com:587"));
    }

    #[tokio::test]
    async fn disabled_mailer_refuses() {
        assert!(matches!(DisabledMailer.send(mail()).await, Err(MailError::Disabled)));
    }

    #[tokio::test]
    async fn log_mailer_accepts_valid_messages() {
        let mailer = LogMailer::new(Sender::new("git@example.com", "").unwrap());
        mailer.send(mail()).await.unwrap();
    }

    #[tokio::test]
    async fn memory_mailer_captures_messages() {
        let mailer = MemoryMailer::new();
        let waiter = {
            let mailer = mailer.clone();
            tokio::spawn(async move { mailer.wait_for(1).await })
        };
        mailer.send(mail()).await.unwrap();
        assert_eq!(waiter.await.unwrap(), vec![mail()]);
        assert_eq!(mailer.sent().len(), 1);

        mailer.fail_sends(true);
        assert!(mailer.send(mail()).await.is_err());
        assert_eq!(mailer.sent().len(), 1);
    }
}
