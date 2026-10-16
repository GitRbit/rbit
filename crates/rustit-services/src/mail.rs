//! Outgoing email delivery (SMTP, log, disabled).
//!
//! Milestone 1 workstream E owns this file.
//!
//! The transport is chosen once from `[mail]`. Requests never wait for a mail server:
//! [`MailService::send`] hands the message to a background task that gives up after the
//! configured timeout and logs failures. Callers that run in the background already can
//! await [`MailService::deliver`] instead.

use std::{fmt, sync::Arc, time::Duration};

use rustit_mail::{DisabledMailer, Mail, MailError, Mailer};

use crate::ServiceContext;

#[derive(Clone)]
pub struct MailService {
    mailer: Arc<dyn Mailer>,
    timeout: Duration,
}

impl fmt::Debug for MailService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailService").field("mailer", &self.mailer).field("timeout", &self.timeout).finish()
    }
}

impl MailService {
    #[allow(clippy::needless_pass_by_value)] // Same constructor shape as every other service.
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let config = &ctx.config.mail;
        let mailer = rustit_mail::from_config(config, &ctx.config.server.site_name).unwrap_or_else(|error| {
            tracing::error!(%error, "email is misconfigured and has been disabled");
            Arc::new(DisabledMailer)
        });
        Self::with_mailer(mailer, Duration::from_secs(config.timeout_seconds.max(1)))
    }

    /// A service delivering through `mailer` (tests use [`rustit_mail::MemoryMailer`]).
    pub fn with_mailer(mailer: Arc<dyn Mailer>, timeout: Duration) -> Self {
        Self { mailer, timeout }
    }

    /// Whether email can be sent at all. Flows that depend on it (password reset,
    /// verification) explain that it is unavailable when this is `false`.
    pub fn is_enabled(&self) -> bool {
        self.mailer.is_enabled()
    }

    /// Queues `mail` for delivery and returns immediately. Failures are logged.
    pub fn send(&self, mail: Mail) {
        let service = self.clone();
        tokio::spawn(async move {
            let _ = service.deliver(mail).await;
        });
    }

    /// Delivers `mail`, waiting at most the configured timeout. Failures are also logged.
    pub async fn deliver(&self, mail: Mail) -> Result<(), MailError> {
        let subject = mail.subject.clone();
        let result = match tokio::time::timeout(self.timeout, self.mailer.send(mail)).await {
            Ok(result) => result,
            Err(_) => Err(MailError::Timeout { seconds: self.timeout.as_secs() }),
        };
        match &result {
            Ok(()) => tracing::debug!(%subject, "email delivered"),
            Err(MailError::Disabled) => tracing::debug!(%subject, "email disabled; message dropped"),
            Err(error) => tracing::warn!(%subject, %error, "email delivery failed"),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use rustit_mail::{MemoryMailer, SendFuture};

    use super::*;

    fn mail() -> Mail {
        Mail { to: "bob@example.com".parse().unwrap(), subject: "Hi".to_owned(), text: String::new(), html: String::new() }
    }

    /// A transport that never answers.
    #[derive(Debug)]
    struct Stalled;

    impl Mailer for Stalled {
        fn send(&self, _mail: Mail) -> SendFuture<'_> {
            Box::pin(std::future::pending())
        }
    }

    #[tokio::test]
    async fn send_returns_immediately_and_delivers_in_the_background() {
        let outbox = MemoryMailer::new();
        let service = MailService::with_mailer(Arc::new(outbox.clone()), Duration::from_secs(5));
        assert!(service.is_enabled());
        service.send(mail());
        let sent = tokio::time::timeout(Duration::from_secs(5), outbox.wait_for(1)).await.unwrap();
        assert_eq!(sent[0].subject, "Hi");
    }

    #[tokio::test]
    async fn delivery_gives_up_after_the_timeout() {
        let service = MailService::with_mailer(Arc::new(Stalled), Duration::from_millis(20));
        assert!(matches!(service.deliver(mail()).await, Err(MailError::Timeout { .. })));
    }

    #[tokio::test]
    async fn disabled_service_reports_it() {
        let service = MailService::with_mailer(Arc::new(DisabledMailer), Duration::from_secs(1));
        assert!(!service.is_enabled());
        assert!(matches!(service.deliver(mail()).await, Err(MailError::Disabled)));
    }
}
