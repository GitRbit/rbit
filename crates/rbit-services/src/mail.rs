//! Outgoing email delivery (SMTP, log, disabled).
//!
//! Milestone 1 workstream E owns this file.
//!
//! The transport is chosen once from `[mail]`. Requests never wait for a mail server:
//! [`MailService::send`] hands the message to a background task that gives up after the
//! configured timeout and logs failures. Callers that run in the background already can
//! await [`MailService::deliver`] instead.

use std::{fmt, sync::Arc, time::Duration};

use rbit_mail::{DisabledMailer, Mail, MailError, Mailer};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::ServiceContext;

/// Bounds both retained messages and simultaneous connections to a stalled relay.
const MAX_PENDING_MAIL: usize = 1024;
const MAX_CONCURRENT_DELIVERIES: usize = 8;

#[derive(Clone)]
pub struct MailService {
    mailer: Arc<dyn Mailer>,
    timeout: Duration,
    pending: Arc<Semaphore>,
    deliveries: Arc<Semaphore>,
}

impl fmt::Debug for MailService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailService").field("mailer", &self.mailer).field("timeout", &self.timeout).finish_non_exhaustive()
    }
}

impl MailService {
    #[allow(clippy::needless_pass_by_value)] // Same constructor shape as every other service.
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let config = &ctx.config.mail;
        let mailer = rbit_mail::from_config(config, &ctx.config.server.site_name).unwrap_or_else(|error| {
            tracing::error!(%error, "email is misconfigured and has been disabled");
            Arc::new(DisabledMailer)
        });
        Self::with_mailer(mailer, Duration::from_secs(config.timeout_seconds.max(1)))
    }

    /// A service delivering through `mailer` (tests use [`rbit_mail::MemoryMailer`]).
    pub fn with_mailer(mailer: Arc<dyn Mailer>, timeout: Duration) -> Self {
        Self {
            mailer,
            timeout,
            pending: Arc::new(Semaphore::new(MAX_PENDING_MAIL)),
            deliveries: Arc::new(Semaphore::new(MAX_CONCURRENT_DELIVERIES)),
        }
    }

    /// Whether email can be sent at all. Flows that depend on it (password reset,
    /// verification) explain that it is unavailable when this is `false`.
    pub fn is_enabled(&self) -> bool {
        self.mailer.is_enabled()
    }

    /// Queues `mail` for delivery and returns immediately. Failures are logged.
    pub fn send(&self, mail: Mail) {
        let Ok(permit) = Arc::clone(&self.pending).try_acquire_owned() else {
            metrics::counter!("rbit_mail_dropped_total", "reason" => "queue_full").increment(1);
            tracing::warn!(subject = %mail.subject, "email delivery skipped: outgoing queue is full");
            return;
        };
        self.spawn_delivery(mail, permit);
    }

    /// Queues account mail with bounded backpressure when the outgoing queue is full.
    /// Delivery itself remains asynchronous; admission gives up after the mail timeout.
    pub async fn queue(&self, mail: Mail) {
        if let Ok(Ok(permit)) = tokio::time::timeout(self.timeout, Arc::clone(&self.pending).acquire_owned()).await {
            self.spawn_delivery(mail, permit);
        } else {
            metrics::counter!("rbit_mail_dropped_total", "reason" => "queue_timeout").increment(1);
            tracing::warn!(subject = %mail.subject, "email delivery skipped: outgoing queue admission timed out");
        }
    }

    fn spawn_delivery(&self, mail: Mail, permit: OwnedSemaphorePermit) {
        let service = self.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let _ = service.deliver(mail).await;
        });
    }

    /// Delivers `mail`, waiting at most the configured timeout. Failures are also logged.
    pub async fn deliver(&self, mail: Mail) -> Result<(), MailError> {
        let subject = mail.subject.clone();
        let deliver = async {
            // Queueing for the relay counts toward the timeout too, so overload cannot
            // accumulate messages waiting indefinitely behind slow deliveries.
            let _permit = self.deliveries.acquire().await.map_err(|_| MailError::Timeout { seconds: self.timeout.as_secs() })?;
            self.mailer.send(mail).await
        };
        let result = match tokio::time::timeout(self.timeout, deliver).await {
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
    use rbit_mail::{MemoryMailer, SendFuture};

    use super::*;

    fn mail() -> Mail {
        Mail {
            to: "bob@example.com".parse().unwrap(),
            subject: "Hi".to_owned(),
            text: String::new(),
            html: String::new(),
            headers: rbit_mail::messages::MailHeaders::default(),
        }
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

    /// A relay whose acknowledgements are controlled by the test.
    #[derive(Debug)]
    struct Gated {
        outbox: MemoryMailer,
        started: Semaphore,
        release: Semaphore,
    }

    impl Mailer for Gated {
        fn send(&self, mail: Mail) -> SendFuture<'_> {
            Box::pin(async move {
                self.started.add_permits(1);
                self.release.acquire().await.unwrap().forget();
                self.outbox.send(mail).await
            })
        }
    }

    #[tokio::test]
    async fn delivery_bounds_connections_and_queue_and_backpressures_account_mail() {
        let relay = Arc::new(Gated { outbox: MemoryMailer::new(), started: Semaphore::new(0), release: Semaphore::new(0) });
        let mut service = MailService::with_mailer(relay.clone(), Duration::from_secs(30));
        service.pending = Arc::new(Semaphore::new(2));
        service.deliveries = Arc::new(Semaphore::new(1));
        let message = |subject: &str| Mail { subject: subject.to_owned(), ..mail() };
        service.send(message("first"));
        service.send(message("second"));
        service.send(message("overloaded"));
        relay.started.acquire().await.unwrap().forget();
        assert!(tokio::time::timeout(Duration::from_millis(20), relay.started.acquire()).await.is_err());

        let mut queued = tokio::spawn(async move { service.queue(Mail { subject: "account".to_owned(), ..mail() }).await });
        assert!(tokio::time::timeout(Duration::from_millis(20), &mut queued).await.is_err());
        relay.release.add_permits(3);
        tokio::time::timeout(Duration::from_secs(5), queued).await.unwrap().unwrap();
        let sent = tokio::time::timeout(Duration::from_secs(5), relay.outbox.wait_for(3)).await.unwrap();
        let mut subjects = sent.iter().map(|mail| mail.subject.as_str()).collect::<Vec<_>>();
        subjects.sort_unstable();
        assert_eq!(subjects, ["account", "first", "second"]);
    }
}
