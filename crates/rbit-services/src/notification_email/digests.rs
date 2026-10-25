//! Daily and weekly digests of notification emails.
//!
//! [`NotificationEmailService::deliver`] queues what a digest user's email would have said
//! (`notification_digest_items`). The notification background task calls
//! [`NotificationEmailService::send_digests`] every few minutes; for every user with queued
//! notifications it works out the period that has started last ([`period_start`]: each
//! day, or each Monday, at the user's digest hour in UTC, or the instance's) and claims the
//! user's digest for it. The claim is one row per user and period, so each period gets at
//! most one digest however often the task runs, and a server restarted mid-run does not
//! send it again. It takes the notifications queued **before** the period started;
//! anything later waits for the next period. Threads read in the app in the meantime are
//! left out.
//!
//! Before sending, everything is checked again: the recipient must still be active, with a
//! verified address and emails on, and still able to read each repository. The digest
//! groups the notifications by repository and thread, carries the same `List-Unsubscribe`
//! headers as single emails and is delivered directly (not through the fire-and-forget
//! queue), so `rbit_notification_digests_total{outcome}` counts what the transport
//! accepted (`sent`) or refused (`failed`). A failed digest is not retried: sending is at
//! most once. Its notifications stay in the inbox.

use std::collections::HashMap;

use rbit_core::{
    AccountId, IssueKind, RepoId, Timestamp,
    config::{EmailDelivery, EmailPreference},
};
use rbit_mail::{
    Mail,
    messages::{DigestEntry, DigestNotification, DigestRepository, DigestThread, MailHeaders},
};
use rbit_storage::sqlite::email_digests::{Claim, ClaimedDigest, DigestItem, DigestState};

use super::{NotificationEmailService, SETTINGS_PATH, message_id_host, thread_segment, wants};
use crate::{ServiceError, notifications::can_read_with_grants};

const HOUR: i64 = 60 * 60;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
/// 1970-01-05, the first Monday after the Unix epoch (a Thursday).
const FIRST_MONDAY: i64 = 4 * DAY;
/// Notifications waiting for a digest are never pruned sooner than this.
const MIN_PENDING_RETENTION: i64 = 2 * WEEK;
/// Users with queued notifications read per page.
const CANDIDATES_PER_PAGE: u32 = 200;

/// What one run of [`NotificationEmailService::send_digests`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DigestRun {
    /// Digests the mail transport accepted.
    pub sent: u64,
    /// Digests the mail transport refused (or that could not be built).
    pub failed: u64,
    /// Digests claimed with nothing left to send (access or preferences changed).
    pub empty: u64,
    /// Queued notifications left out because their thread was read in the app.
    pub skipped_read: u64,
}

/// The start of the digest period `now` falls in: the latest day (daily) or Monday
/// (weekly) at `hour`:00 UTC that is not after `now`. Immediate delivery uses daily
/// periods for what was queued before the user switched back.
pub fn period_start(delivery: EmailDelivery, hour: u8, now: Timestamp) -> Timestamp {
    let hour = i64::from(hour.min(rbit_core::config::MAX_DIGEST_HOUR)) * HOUR;
    let (length, origin) = match delivery {
        EmailDelivery::Weekly => (WEEK, FIRST_MONDAY + hour),
        EmailDelivery::Daily | EmailDelivery::Immediate => (DAY, hour),
    };
    Timestamp(now.0.saturating_sub(origin).div_euclid(length).saturating_mul(length).saturating_add(origin))
}

/// Where pruning stops for digests: the inbox retention, but never less than two weeks.
pub(super) fn prune_cutoff(retention_cutoff: Timestamp, now: Timestamp) -> Timestamp {
    Timestamp(retention_cutoff.0.min(now.0.saturating_sub(MIN_PENDING_RETENTION)))
}

impl NotificationEmailService {
    /// Sends every digest that is due now (see the module documentation).
    pub async fn send_digests(&self) -> Result<DigestRun, ServiceError> {
        let mut run = DigestRun::default();
        if !self.available() {
            return Ok(run);
        }
        let now = self.now();
        let store = self.ctx.database.email_digests();
        let default_hour = self.default_digest_hour();
        let mut after = AccountId(0);
        loop {
            let page = store.candidates(after, CANDIDATES_PER_PAGE).await?;
            let Some(last) = page.last() else { break };
            after = last.account;
            for candidate in &page {
                let period = period_start(candidate.delivery, candidate.digest_hour.unwrap_or(default_hour), now);
                let claimed = match store.claim(candidate.account, period, now).await? {
                    Claim::Taken => continue,
                    Claim::Nothing { skipped_read } => {
                        run.skipped_read += skipped_read;
                        continue;
                    }
                    Claim::Claimed { digest, skipped_read } => {
                        run.skipped_read += skipped_read;
                        digest
                    }
                };
                let weekly = candidate.delivery == EmailDelivery::Weekly;
                let state = match self.send_digest(&claimed, weekly).await {
                    Ok(state) => state,
                    Err(error) => {
                        tracing::warn!(%error, account = claimed.account.0, "cannot build a notification digest");
                        DigestState::Failed
                    }
                };
                match state {
                    DigestState::Sent => run.sent += 1,
                    DigestState::Empty => run.empty += 1,
                    DigestState::Failed | DigestState::Claimed => run.failed += 1,
                }
                if matches!(state, DigestState::Sent | DigestState::Failed) {
                    let outcome = if state == DigestState::Sent { "sent" } else { "failed" };
                    metrics::counter!("rbit_notification_digests_total", "outcome" => outcome).increment(1);
                }
                store.finish(claimed.id, state, self.now()).await?;
            }
            if page.len() < usize::try_from(CANDIDATES_PER_PAGE).unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(run)
    }

    /// Checks, renders and delivers one claimed digest; returns what became of it.
    async fn send_digest(&self, claimed: &ClaimedDigest, weekly: bool) -> Result<DigestState, ServiceError> {
        let database = &self.ctx.database;
        let Some(recipient) = database.notification_email().recipients(&[claimed.account]).await?.into_iter().next() else {
            // Suspended, deleted or no longer verified.
            return Ok(DigestState::Empty);
        };
        let preference = recipient.preference.unwrap_or(self.ctx.config.notifications.default_email);
        if preference == EmailPreference::Off {
            return Ok(DigestState::Empty);
        }
        let user = recipient.user;
        let items: Vec<_> =
            database.email_digests().items(claimed.id).await?.into_iter().filter(|item| wants(preference, item.reason)).collect();

        // Read access is checked again now, once per repository.
        let mut readable = HashMap::<RepoId, bool>::new();
        for repo in items.iter().map(|item| item.repo) {
            if readable.contains_key(&repo) {
                continue;
            }
            let can_read = match database.admin().find_repository(repo).await? {
                Some(owned) => {
                    let grants = if user.is_admin || user.id() == owned.repo.owner_id {
                        HashMap::new()
                    } else {
                        database.repositories().grants_for(repo, owned.repo.owner_id, &[user.id()]).await?
                    };
                    can_read_with_grants(&user, &owned, grants.get(&user.id()))
                }
                None => false,
            };
            readable.insert(repo, can_read);
        }
        let items: Vec<_> = items.into_iter().filter(|item| readable.get(&item.repo).copied().unwrap_or(false)).collect();
        if items.is_empty() {
            return Ok(DigestState::Empty);
        }

        let base_url = self.ctx.config.server.base_url();
        let unsubscribe_link = self.unsubscribe_link(user.id()).await?;
        let message = DigestNotification {
            site_name: self.instance.current().site_name,
            base_url: base_url.to_owned(),
            weekly,
            repositories: group(base_url, &items),
            settings_link: format!("{base_url}{SETTINGS_PATH}"),
            unsubscribe_link: unsubscribe_link.clone(),
        };
        let headers = MailHeaders {
            // Stable per user and period, so even a duplicate would be recognised.
            message_id: Some(format!("<digest/{}/{}@{}>", user.id().0, claimed.period.0, message_id_host(base_url))),
            in_reply_to: None,
            one_click_unsubscribe: Some(unsubscribe_link),
        };
        let mail = Mail::render(user.email.clone(), &message).map_err(ServiceError::internal)?.with_headers(headers);
        Ok(match self.mail.deliver(mail).await {
            Ok(()) => {
                tracing::info!(user = %user.name(), notifications = items.len(), "notification digest sent");
                DigestState::Sent
            }
            Err(error) => {
                tracing::warn!(%error, user = %user.name(), "notification digest not sent");
                DigestState::Failed
            }
        })
    }
}

/// `items` (sorted by repository, thread and time) as the digest's sections.
fn group(base_url: &str, items: &[DigestItem]) -> Vec<DigestRepository> {
    let mut repositories: Vec<DigestRepository> = Vec::new();
    let mut last: Option<(RepoId, rbit_core::IssueId)> = None;
    for item in items {
        let full_name = format!("{}/{}", item.owner, item.repo_name);
        let thread_path = format!("{full_name}/{}/{}", thread_segment(item.kind), item.number.get());
        if last.map(|(repo, _)| repo) != Some(item.repo) {
            repositories.push(DigestRepository { name: full_name.clone(), link: format!("{base_url}/{full_name}"), threads: Vec::new() });
        }
        let Some(repository) = repositories.last_mut() else { continue };
        if last != Some((item.repo, item.issue)) {
            repository.threads.push(DigestThread {
                number: u64::from(item.number.get()),
                title: item.title.clone(),
                is_pull: item.kind == IssueKind::Pull,
                link: format!("{base_url}/{thread_path}"),
                entries: Vec::new(),
            });
        }
        last = Some((item.repo, item.issue));
        let Some(thread) = repository.threads.last_mut() else { continue };
        let link = match &item.anchor {
            Some(anchor) => format!("{base_url}/{thread_path}#{anchor}"),
            None => format!("{base_url}/{thread_path}"),
        };
        thread.entries.push(DigestEntry { headline: item.headline.clone(), excerpt: item.excerpt.clone(), link });
    }
    repositories
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-19 00:00 UTC, a Monday.
    const MONDAY: i64 = 1_792_368_000;

    #[test]
    fn the_reference_monday_is_a_monday() {
        // 2026-10-19 is 20 745 days after the epoch; (20 745 + 3) % 7 == 0 means Monday.
        assert_eq!(MONDAY % DAY, 0);
        assert_eq!((MONDAY / DAY + 3) % 7, 0);
        assert_eq!((FIRST_MONDAY / DAY + 3) % 7, 0);
    }

    #[test]
    fn daily_periods_start_at_the_digest_hour() {
        let at = |seconds: i64| period_start(EmailDelivery::Daily, 8, Timestamp(MONDAY + seconds));
        assert_eq!(at(8 * HOUR), Timestamp(MONDAY + 8 * HOUR), "the boundary itself starts the period");
        assert_eq!(at(8 * HOUR - 1), Timestamp(MONDAY - DAY + 8 * HOUR), "a second before belongs to the day before");
        assert_eq!(at(23 * HOUR), Timestamp(MONDAY + 8 * HOUR));
        assert_eq!(at(DAY + 7 * HOUR), Timestamp(MONDAY + 8 * HOUR));
        assert_eq!(period_start(EmailDelivery::Daily, 0, Timestamp(MONDAY + 1)), Timestamp(MONDAY));
        assert_eq!(period_start(EmailDelivery::Immediate, 8, Timestamp(MONDAY + 9 * HOUR)), Timestamp(MONDAY + 8 * HOUR));
        assert_eq!(
            period_start(EmailDelivery::Daily, 99, Timestamp(MONDAY + 23 * HOUR)),
            Timestamp(MONDAY + 23 * HOUR),
            "hours are clamped"
        );
    }

    #[test]
    fn weekly_periods_start_on_mondays() {
        let at = |seconds: i64| period_start(EmailDelivery::Weekly, 8, Timestamp(MONDAY + seconds));
        assert_eq!(at(8 * HOUR), Timestamp(MONDAY + 8 * HOUR));
        assert_eq!(at(6 * DAY + 23 * HOUR), Timestamp(MONDAY + 8 * HOUR), "Sunday night is still the same week");
        assert_eq!(at(7 * DAY + 8 * HOUR), Timestamp(MONDAY + WEEK + 8 * HOUR));
        assert_eq!(at(8 * HOUR - 1), Timestamp(MONDAY - WEEK + 8 * HOUR), "Monday before the hour belongs to last week");
        assert_eq!(period_start(EmailDelivery::Weekly, 0, Timestamp(0)), Timestamp(FIRST_MONDAY - WEEK), "before the first Monday");
    }

    #[test]
    fn pending_notifications_are_kept_for_two_weeks() {
        let now = Timestamp(MONDAY);
        assert_eq!(prune_cutoff(Timestamp(MONDAY - DAY), now), Timestamp(MONDAY - 2 * WEEK));
        assert_eq!(prune_cutoff(Timestamp(MONDAY - 90 * DAY), now), Timestamp(MONDAY - 90 * DAY));
    }
}
