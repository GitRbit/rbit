//! Daily and weekly digests of notification emails.
//!
//! [`NotificationEmailService::deliver`] queues what a digest user's email would have said
//! (`notification_digest_items`). The notification background task calls
//! [`NotificationEmailService::send_digests`] every few minutes. Periods, the one claim
//! per user and period, the re-checks before sending and the grouping are feature policy
//! ([`rbit_notifications::mail::digest`]); this module composes the stores, templates and
//! the direct transport, so `rbit_notification_digests_total{outcome}` counts what the
//! transport accepted (`sent`) or refused (`failed`).

use rbit_core::{Timestamp, config::EmailDelivery};
pub use rbit_notifications::mail::digest::DigestRun;
use rbit_notifications::mail::digest::{DigestDefaults, send_digests};

use super::{NotificationEmailService, outbox};
use crate::ServiceError;

/// The start of the digest period `now` falls in: the latest day (daily) or Monday
/// (weekly) at `hour`:00 UTC that is not after `now`. Immediate delivery uses daily
/// periods for what was queued before the user switched back.
pub fn period_start(delivery: EmailDelivery, hour: u8, now: Timestamp) -> Timestamp {
    rbit_notifications::mail::digest::period_start(outbox::cadence(delivery), hour, now)
}

impl NotificationEmailService {
    /// Sends every digest that is due now (see the module documentation).
    pub async fn send_digests(&self) -> Result<DigestRun, ServiceError> {
        if !self.available() {
            return Ok(DigestRun::default());
        }
        let defaults = DigestDefaults {
            preference: outbox::preference(self.ctx.config.notifications.default_email),
            hour: self.default_digest_hour(),
            base_url: self.ctx.config.server.base_url(),
        };
        send_digests(&outbox::DigestView(self), defaults, || self.now()).await
    }
}
