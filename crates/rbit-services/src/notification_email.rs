//! Email notifications: the in-app notifications of [`crate::NotificationService`], also
//! sent by email.
//!
//! Milestone 3 workstream N (email notifications) owns this file.
//!
//! The inbox fan-out decides who hears about an event. For the rows it writes it hands the
//! recipients to [`NotificationEmailService::deliver`], which mails each one who
//!
//! * has a **verified** address (unverified addresses never receive notification email),
//! * wants it: their [`EmailPreference`] (`[notifications] default_email` until they
//!   choose); `participating` leaves out threads only subscribed to by hand,
//! * is **not the actor** (the fan-out already excludes them; checked again),
//! * can **still read the repository** now (checked again at send time, with grants),
//! * was **not mailed about this activity before** (a dedup key per recipient, thread and
//!   activity, claimed in `notification_emails`),
//! * is within their [`crate::ThrottleScope::NotificationEmail`] budget.
//!
//! Delivery goes through the shared [`MailService`] queue. Nothing is sent when the
//! transport is `disabled`, with `[notifications] email = false`, or with the inbox off.
//!
//! **Digests.** Users whose [`EmailDelivery`] is daily or weekly get no email per
//! notification: what the email would have said is queued instead (after the same checks
//! and the same dedup claim, but without spending the throttle budget), and [`digests`]
//! sends it as one email per period, grouped by repository.
//!
//! **Threading.** Every message about a thread references `<owner/repo/issues/N@host>`
//! (`pulls` for pull requests): the opening message uses it as its own `Message-ID`, the
//! others set `In-Reply-To` and `References` to it. `Reply-To` is never set: rbit receives
//! no mail.
//!
//! **One-click unsubscribe.** Every message carries `List-Unsubscribe` and
//! `List-Unsubscribe-Post` (RFC 8058) pointing at `/email/unsubscribe/{token}`, where the
//! token is `{account id}.{HMAC-SHA256 of the id}` keyed with a server secret generated on
//! first use (`server_secrets`): unforgeable, and nothing to store per message. Using it
//! turns notification emails off for that account.

mod digests;
mod outbox;

use std::{fmt, sync::Arc};

pub use digests::{DigestRun, period_start};
use rbit_core::{
    AccountId, CommentSource, Email, IssueId, RepoId, Timestamp, User,
    config::{EmailDelivery, EmailPreference},
    issue::{CommentAction, IssueAction, ReviewAction, ReviewState},
};
use rbit_notifications::NotificationReason;
pub use rbit_notifications::mail::policy::{SETTINGS_PATH, UNSUBSCRIBE_PATH};
use rbit_notifications::mail::{
    activity::{MailEvent, MailThread, ReviewVerdict, describe},
    delivery::{MailScope, deliver},
    policy::valid_digest_hour,
    unsubscribe::{LINK_KEY_NAME, sign_link, verify_link},
};
use rbit_storage::sqlite::notification_email::DeliverySettings;
use tokio::sync::OnceCell;

use crate::{DomainEvent, InstanceService, MailService, ServiceContext, ServiceError, ThrottleService};

/// A user's notification email settings, for the settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailSettings {
    pub preference: EmailPreference,
    /// Whether the user chose it (otherwise it is the configured default).
    pub chosen: bool,
    /// Immediately or in daily or weekly digests.
    pub delivery: EmailDelivery,
    /// The hour (UTC) the user chose for digests; `None` follows `default_digest_hour`.
    pub digest_hour: Option<u8>,
    /// The instance's digest hour (UTC).
    pub default_digest_hour: u8,
    pub address: Email,
    /// Unverified addresses receive no notification email whatever the preference.
    pub verified: bool,
    /// Whether this server sends notification emails at all.
    pub available: bool,
}

/// Where the service reads the time: the system clock, or a test's.
#[derive(Clone)]
struct Clock(Arc<dyn Fn() -> Timestamp + Send + Sync>);

impl fmt::Debug for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Clock")
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self(Arc::new(Timestamp::now))
    }
}

#[derive(Debug, Clone)]
pub struct NotificationEmailService {
    ctx: ServiceContext,
    mail: MailService,
    throttle: ThrottleService,
    instance: InstanceService,
    /// The link-signing key, read (or created) once.
    link_key: Arc<OnceCell<Vec<u8>>>,
    clock: Clock,
}

impl NotificationEmailService {
    pub(crate) fn new(ctx: ServiceContext, mail: MailService, throttle: ThrottleService, instance: InstanceService) -> Self {
        Self { ctx, mail, throttle, instance, link_key: Arc::default(), clock: Clock::default() }
    }

    /// The same service delivering through `mail` (tests use a memory transport).
    #[must_use]
    pub fn with_mail(&self, mail: MailService) -> Self {
        Self { mail, ..self.clone() }
    }

    /// The same service reading the time from `clock` (tests move it across digest
    /// periods). It stamps queued notifications and decides which digests are due.
    #[must_use]
    pub fn with_clock(&self, clock: impl Fn() -> Timestamp + Send + Sync + 'static) -> Self {
        Self { clock: Clock(Arc::new(clock)), ..self.clone() }
    }

    fn now(&self) -> Timestamp {
        (self.clock.0)()
    }

    /// The hour (UTC) digests are sent at for users who did not choose one: the runtime
    /// instance setting (`[notifications] digest_hour` until an administrator changes it).
    pub fn default_digest_hour(&self) -> u8 {
        self.instance.current().digest_hour
    }

    /// Whether notification emails are sent at all: `[notifications] email`, the inbox
    /// (`[issues] notifications`) and a mail transport.
    pub fn available(&self) -> bool {
        self.ctx.config.notifications.email && self.ctx.config.issues.notifications && self.mail.is_enabled()
    }

    // ------------------------------------------------------------------ preferences

    /// `user`'s settings for the settings page.
    pub async fn settings(&self, user: &User) -> Result<EmailSettings, ServiceError> {
        let account = self.ctx.database.recovery().find_account(user.id()).await?.ok_or(ServiceError::NotFound)?;
        let store = self.ctx.database.notification_email();
        let chosen = store.preference(user.id()).await?;
        let delivery = store.delivery(user.id()).await?;
        Ok(EmailSettings {
            preference: chosen.unwrap_or(self.ctx.config.notifications.default_email),
            chosen: chosen.is_some(),
            delivery: delivery.delivery,
            digest_hour: delivery.digest_hour,
            default_digest_hour: self.default_digest_hour(),
            address: account.email,
            verified: account.email_verified_at.is_some(),
            available: self.available(),
        })
    }

    /// Stores `user`'s choice.
    pub async fn set_preference(&self, user: &User, preference: EmailPreference) -> Result<(), ServiceError> {
        Ok(self.ctx.database.notification_email().set_preference(user.id(), preference, Timestamp::now()).await?)
    }

    /// Stores `user`'s choice of what to email and how to deliver it. Digest hours are
    /// 0–23 (UTC). Notifications already queued for a digest go out with the next one,
    /// even after switching back to immediate emails.
    pub async fn set_preferences(
        &self,
        user: &User,
        preference: EmailPreference,
        delivery: EmailDelivery,
        digest_hour: Option<u8>,
    ) -> Result<(), ServiceError> {
        if !valid_digest_hour(digest_hour) {
            return Err(ServiceError::Invalid("the digest hour must be between 0 and 23".to_owned()));
        }
        let settings = DeliverySettings { delivery, digest_hour };
        Ok(self.ctx.database.notification_email().save(user.id(), preference, settings, Timestamp::now()).await?)
    }

    // ------------------------------------------------------------------ one-click unsubscribe

    /// The absolute unsubscribe link of `account` (also its `List-Unsubscribe` target).
    pub async fn unsubscribe_link(&self, account: AccountId) -> Result<String, ServiceError> {
        let token = sign_link(self.link_key().await?, account);
        Ok(format!("{}{UNSUBSCRIBE_PATH}{token}", self.ctx.config.server.base_url()))
    }

    /// The username an unsubscribe token belongs to; [`ServiceError::NotFound`] for tokens
    /// that are malformed, forged or of a deleted account.
    pub async fn unsubscribe_target(&self, token: &str) -> Result<String, ServiceError> {
        let account = verify_link(self.link_key().await?, token).ok_or(ServiceError::NotFound)?;
        let user = self.ctx.database.accounts().find_user(account).await?.ok_or(ServiceError::NotFound)?;
        Ok(user.name().to_string())
    }

    /// Turns notification emails off for the account `token` belongs to and returns its
    /// username. Repeating it is harmless.
    pub async fn unsubscribe(&self, token: &str) -> Result<String, ServiceError> {
        let account = verify_link(self.link_key().await?, token).ok_or(ServiceError::NotFound)?;
        let user = self.ctx.database.accounts().find_user(account).await?.ok_or(ServiceError::NotFound)?;
        self.set_preference(&user, EmailPreference::Off).await?;
        tracing::info!(user = %user.name(), "notification emails turned off by unsubscribe link");
        Ok(user.name().to_string())
    }

    async fn link_key(&self) -> Result<&[u8], ServiceError> {
        let key = self
            .link_key
            .get_or_try_init(|| async {
                let mut fresh = [0_u8; 32];
                getrandom::fill(&mut fresh).map_err(|error| ServiceError::internal(std::io::Error::other(error.to_string())))?;
                Ok::<_, ServiceError>(self.ctx.database.server_secrets().get_or_insert(LINK_KEY_NAME, &fresh).await?)
            })
            .await?;
        Ok(key.as_slice())
    }

    // ------------------------------------------------------------------ delivery

    /// Mails `recipients` (each with the reason the inbox recorded) about `event` on thread
    /// `issue` of `repo` through the feature delivery policy, as the module describes.
    pub(crate) async fn deliver(
        &self,
        event: &DomainEvent,
        repo: RepoId,
        issue: IssueId,
        actor: Option<AccountId>,
        recipients: &[(AccountId, NotificationReason)],
    ) -> Result<(), ServiceError> {
        if recipients.is_empty() || !self.available() {
            return Ok(());
        }
        let database = &self.ctx.database;
        let Some(thread) = database.issues().find(issue).await? else { return Ok(()) };
        let actor_name = match actor {
            Some(actor) => database.accounts().find_accounts(&[actor]).await?.into_iter().next().map(|account| account.name.to_string()),
            None => None,
        };
        let now = self.now();
        let Some(activity) = self.activity(event, &thread, actor_name.as_deref(), now).await? else { return Ok(()) };
        let default = outbox::preference(self.ctx.config.notifications.default_email);
        let scope = MailScope { repo, issue, actor, now, default };
        deliver(&outbox::ThreadOutbox::new(self, &thread), scope, &activity, recipients).await
    }

    /// Deletes outbox rows and digests created before `cutoff` (the inbox's retention).
    /// Notifications still waiting for a digest are kept for at least two weeks.
    pub(crate) async fn prune(&self, cutoff: Timestamp) -> Result<u64, ServiceError> {
        outbox::prune(&self.ctx.database, cutoff, self.now()).await
    }

    /// What `event` did to `thread`, or `None` for events that send no email. Comment
    /// and review rows are read here; the wording and dedup key are feature policy.
    async fn activity(
        &self,
        event: &DomainEvent,
        thread: &rbit_core::Issue,
        actor: Option<&str>,
        now: Timestamp,
    ) -> Result<Option<rbit_notifications::mail::activity::MailActivity>, ServiceError> {
        let thread = MailThread { kind: thread.kind, number: thread.number.get(), body: thread.body.as_str() };
        Ok(match event {
            DomainEvent::Issue { action, .. } => match action {
                IssueAction::Opened => Some(describe(MailEvent::Opened, thread, actor, now)),
                IssueAction::Edited { previous_body: Some(_), .. } => Some(describe(MailEvent::BodyEdited, thread, actor, now)),
                IssueAction::Assigned { assignee } => Some(describe(MailEvent::Assigned(*assignee), thread, actor, now)),
                IssueAction::ReviewRequested { reviewer } => Some(describe(MailEvent::ReviewRequested(*reviewer), thread, actor, now)),
                IssueAction::Closed { reason } => Some(describe(MailEvent::Closed(*reason), thread, actor, now)),
                IssueAction::Reopened => Some(describe(MailEvent::Reopened, thread, actor, now)),
                _ => None,
            },
            DomainEvent::IssueComment { issue, comment, action, .. } => {
                if matches!(action, CommentAction::Deleted { .. }) {
                    return Ok(None);
                }
                let Some(row) = self.ctx.database.issues().find_comment(*issue, *comment).await? else { return Ok(None) };
                let comment_event = MailEvent::Comment {
                    id: *comment,
                    edited: matches!(action, CommentAction::Edited { .. }),
                    by_actions: row.source == CommentSource::Actions,
                    body: row.body.as_str(),
                };
                Some(describe(comment_event, thread, actor, now))
            }
            DomainEvent::PullRequestReview { pull, review, action: ReviewAction::Submitted, .. } => {
                let Some(row) = self.ctx.database.reviews().find(*pull, *review).await? else { return Ok(None) };
                let verdict = match row.state {
                    ReviewState::Approved => ReviewVerdict::Approved,
                    ReviewState::ChangesRequested => ReviewVerdict::ChangesRequested,
                    _ => ReviewVerdict::Commented,
                };
                Some(describe(MailEvent::Review { id: review.0, verdict, body: &row.body }, thread, actor, now))
            }
            _ => None,
        })
    }
}
