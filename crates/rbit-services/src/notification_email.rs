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
//! * is within their [`ThrottleScope::NotificationEmail`] budget.
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

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::Arc,
};

pub use digests::{DigestRun, period_start};
use hmac::{Hmac, KeyInit, Mac};
use rbit_core::{
    AccountId, Email, Issue, IssueId, IssueKind, RepoId, StateReason, Timestamp, User,
    config::{EmailDelivery, EmailPreference},
    issue::{CommentAction, IssueAction, ReviewAction, ReviewState},
    repository::OwnedRepository,
};
use rbit_mail::{
    Mail,
    messages::{MailHeaders, ThreadNotification},
};
use rbit_storage::sqlite::{
    email_digests::NewDigestItem,
    notification_email::DeliverySettings,
    notifications::{NOTIFICATION_BATCH_SIZE, NotificationReason},
};
use sha2::Sha256;
use tokio::sync::OnceCell;

use crate::{
    DomainEvent, InstanceService, MailService, ServiceContext, ServiceError, ThrottleScope, ThrottleService,
    notifications::can_read_with_grants,
};

/// The `server_secrets` row whose key signs unsubscribe links.
const LINK_KEY_NAME: &str = "notification_unsubscribe";
/// What the link MAC covers besides the account id, so the key can sign other things later.
const LINK_PURPOSE: &str = "rbit/notification-unsubscribe/v1";
/// Longest excerpt quoted in a message, in characters.
const MAX_EXCERPT_CHARS: usize = 1000;
/// Longest excerpt of one notification in a digest, in characters (on one line).
const MAX_DIGEST_EXCERPT_CHARS: usize = 200;
/// Where users manage their notification emails.
pub const SETTINGS_PATH: &str = "/settings/notifications";
/// The one-click unsubscribe endpoint; the token follows.
pub const UNSUBSCRIBE_PATH: &str = "/email/unsubscribe/";

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

/// What an event did, as one email describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Activity {
    /// The dedup key, unique per thread and activity; also ends the `Message-ID`.
    key: String,
    headline: String,
    excerpt: String,
    /// The fragment of the thread page to link to.
    anchor: Option<String>,
    /// Whether this message starts the thread (its `Message-ID` is the thread's).
    opening: bool,
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
        if digest_hour.is_some_and(|hour| hour > rbit_core::config::MAX_DIGEST_HOUR) {
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
    /// `issue` of `repo`, as described in the module documentation.
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
        let store = database.notification_email();
        let default = self.ctx.config.notifications.default_email;

        let mut unique = BTreeMap::<AccountId, NotificationReason>::new();
        for &(user, reason) in recipients.iter().filter(|(user, _)| Some(*user) != actor) {
            unique.entry(user).and_modify(|current| *current = (*current).max(reason)).or_insert(reason);
        }
        let recipients: Vec<_> = unique.into_iter().collect();
        for batch in recipients.chunks(NOTIFICATION_BATCH_SIZE) {
            // Ownership or visibility can change while earlier batches are delivered.
            let Some(owned) = database.admin().find_repository(repo).await? else { break };
            let ids: Vec<_> = batch.iter().map(|(user, _)| *user).collect();
            // This fresh read includes suspension, verification and preference. Inbox
            // authorization is deliberately not reused for external delivery.
            let users = store.recipients(&ids).await?;
            let reasons: HashMap<_, _> = batch.iter().copied().collect();
            let users: Vec<_> = users
                .into_iter()
                .filter_map(|recipient| {
                    let reason = *reasons.get(&recipient.user.id())?;
                    wants(recipient.preference.unwrap_or(default), reason).then_some((recipient.user, reason, recipient.delivery))
                })
                .collect();
            let grant_ids: Vec<_> = users
                .iter()
                .map(|(user, _, _)| user)
                .filter(|user| !user.is_admin && user.id() != owned.repo.owner_id)
                .map(User::id)
                .collect();
            let grants = database.repositories().grants_for(repo, owned.repo.owner_id, &grant_ids).await?;
            let users: Vec<_> =
                users.into_iter().filter(|(user, _, _)| can_read_with_grants(user, &owned, grants.get(&user.id()))).collect();
            // Rendering may fail (including initializing the unsubscribe signing key).
            // Finish the bounded batch before claiming any of its delivery keys.
            let mut rendered = Vec::with_capacity(users.len());
            let mut digest = Vec::new();
            for (user, reason, delivery) in users {
                if delivery.is_digest() {
                    digest.push((user.id(), reason));
                } else {
                    let mail = self.render(&owned, &thread, &activity, reason, user.email.clone(), user.id()).await?;
                    rendered.push((user, mail));
                }
            }
            let ids: Vec<_> = rendered.iter().map(|(user, _)| user.id()).chain(digest.iter().map(|(user, _)| *user)).collect();
            // One claim for both: switching between immediate and digest delivery never
            // sends an activity twice.
            let claimed = store.claim_many(&ids, issue, &activity.key, now).await?;
            let queued: Vec<_> = digest
                .into_iter()
                .filter(|(user, _)| claimed.contains(user))
                .map(|(account, reason)| NewDigestItem {
                    account,
                    repo,
                    issue,
                    reason,
                    headline: activity.headline.clone(),
                    excerpt: one_line_excerpt(&activity.excerpt),
                    anchor: activity.anchor.clone(),
                    created_at: now,
                })
                .collect();
            database.email_digests().queue(&queued).await?;
            for (user, mail) in rendered.into_iter().filter(|(user, _)| claimed.contains(&user.id())) {
                if let Err(error) = self.throttle.check(ThrottleScope::NotificationEmail, &user.email.as_str().to_lowercase()) {
                    tracing::info!(%error, user = %user.name(), "notification email skipped: too many recently");
                    continue;
                }
                self.mail.send(mail);
            }
        }
        Ok(())
    }

    /// Deletes outbox rows and digests created before `cutoff` (the inbox's retention).
    /// Notifications still waiting for a digest are kept for at least two weeks.
    pub(crate) async fn prune(&self, cutoff: Timestamp) -> Result<u64, ServiceError> {
        let outbox = self.ctx.database.notification_email().prune(cutoff).await?;
        let digests = self.ctx.database.email_digests().prune(digests::prune_cutoff(cutoff, self.now())).await?;
        Ok(outbox + digests)
    }

    /// What `event` did to `thread`, or `None` for events that send no email.
    async fn activity(
        &self,
        event: &DomainEvent,
        thread: &Issue,
        actor: Option<&str>,
        now: Timestamp,
    ) -> Result<Option<Activity>, ServiceError> {
        let what = format!("{} #{}", kind_label(thread.kind), thread.number.get());
        let who = actor.unwrap_or("Someone");
        let stamped = |name: &str| format!("{name}-{}", now.seconds());
        let activity = |key: String, headline: String, excerpt: &str, anchor: Option<String>, opening: bool| Activity {
            key,
            headline,
            excerpt: excerpt_of(excerpt),
            anchor,
            opening,
        };
        Ok(match event {
            DomainEvent::Issue { action, .. } => match action {
                IssueAction::Opened => Some(activity("opened".into(), format!("{who} opened {what}."), thread.body.as_str(), None, true)),
                // Only users newly mentioned in the description receive it.
                IssueAction::Edited { previous_body: Some(_), .. } => {
                    Some(activity("opened".into(), format!("{who} mentioned you in {what}."), thread.body.as_str(), None, true))
                }
                IssueAction::Assigned { assignee } => {
                    Some(activity(stamped(&format!("assigned-{}", assignee.0)), format!("{who} assigned you to {what}."), "", None, false))
                }
                IssueAction::ReviewRequested { reviewer } => Some(activity(
                    stamped(&format!("review-requested-{}", reviewer.0)),
                    format!("{who} requested your review on {what}."),
                    "",
                    None,
                    false,
                )),
                IssueAction::Closed { reason } => {
                    let headline = match (actor, reason) {
                        (None, StateReason::Merged) => capitalised(&format!("{what} was merged.")),
                        (None, _) => capitalised(&format!("{what} was closed.")),
                        (Some(actor), StateReason::Merged) => format!("{actor} merged {what}."),
                        (Some(actor), StateReason::Completed) => format!("{actor} closed {what} as completed."),
                        (Some(actor), StateReason::NotPlanned) => format!("{actor} closed {what} as not planned."),
                    };
                    let name = if *reason == StateReason::Merged { "merged" } else { "closed" };
                    Some(activity(stamped(name), headline, "", None, false))
                }
                IssueAction::Reopened => Some(activity(stamped("reopened"), format!("{who} reopened {what}."), "", None, false)),
                _ => None,
            },
            DomainEvent::IssueComment { issue, comment, action, .. } => {
                let headline = match action {
                    CommentAction::Created => format!("{who} commented on {what}."),
                    // Only users newly mentioned by the edit receive it.
                    CommentAction::Edited { .. } => format!("{who} mentioned you in a comment on {what}."),
                    CommentAction::Deleted { .. } => return Ok(None),
                };
                let Some(row) = self.ctx.database.issues().find_comment(*issue, *comment).await? else { return Ok(None) };
                Some(activity(
                    format!("comment-{}", comment.0),
                    headline,
                    row.body.as_str(),
                    Some(format!("issuecomment-{}", comment.0)),
                    false,
                ))
            }
            DomainEvent::PullRequestReview { pull, review, action: ReviewAction::Submitted, .. } => {
                let Some(row) = self.ctx.database.reviews().find(*pull, *review).await? else { return Ok(None) };
                let headline = match row.state {
                    ReviewState::Approved => format!("{who} approved {what}."),
                    ReviewState::ChangesRequested => format!("{who} requested changes on {what}."),
                    _ => format!("{who} reviewed {what}."),
                };
                Some(activity(format!("review-{}", review.0), headline, &row.body, Some(format!("pullrequestreview-{}", review.0)), false))
            }
            _ => None,
        })
    }

    async fn render(
        &self,
        repo: &OwnedRepository,
        thread: &Issue,
        activity: &Activity,
        reason: NotificationReason,
        to: Email,
        recipient: AccountId,
    ) -> Result<Mail, ServiceError> {
        let base_url = self.ctx.config.server.base_url();
        let full_name = repo.full_name();
        let path = format!("{full_name}/{}/{}", thread_segment(thread.kind), thread.number.get());
        let link = match &activity.anchor {
            Some(anchor) => format!("{base_url}/{path}#{anchor}"),
            None => format!("{base_url}/{path}"),
        };
        let unsubscribe_link = self.unsubscribe_link(recipient).await?;
        let message = ThreadNotification {
            site_name: self.instance.current().site_name,
            base_url: base_url.to_owned(),
            repository: full_name,
            number: u64::from(thread.number.get()),
            title: thread.title.to_string(),
            headline: activity.headline.clone(),
            excerpt: activity.excerpt.clone(),
            link,
            is_pull: thread.kind == IssueKind::Pull,
            is_reply: !activity.opening,
            reason: format!("You are receiving this because {}.", reason_clause(reason)),
            settings_link: format!("{base_url}{SETTINGS_PATH}"),
            unsubscribe_link: unsubscribe_link.clone(),
        };
        let host = message_id_host(base_url);
        let root = format!("<{path}@{host}>");
        let headers = if activity.opening {
            MailHeaders { message_id: Some(root), in_reply_to: None, one_click_unsubscribe: Some(unsubscribe_link) }
        } else {
            MailHeaders {
                message_id: Some(format!("<{path}/{}@{host}>", activity.key)),
                in_reply_to: Some(root),
                one_click_unsubscribe: Some(unsubscribe_link),
            }
        };
        Ok(Mail::render(to, &message).map_err(ServiceError::internal)?.with_headers(headers))
    }
}

/// Whether `preference` asks for email about a notification with `reason`.
fn wants(preference: EmailPreference, reason: NotificationReason) -> bool {
    match preference {
        EmailPreference::Off => false,
        EmailPreference::Participating => reason != NotificationReason::Manual,
        EmailPreference::All => true,
    }
}

/// The end of "You are receiving this because …".
const fn reason_clause(reason: NotificationReason) -> &'static str {
    match reason {
        NotificationReason::Manual => "you are subscribed to this thread",
        NotificationReason::Comment => "you commented on this thread",
        NotificationReason::Reviewed => "you reviewed this pull request",
        NotificationReason::Author => "you opened this thread",
        NotificationReason::Mention => "you were mentioned",
        NotificationReason::Assigned => "you are assigned",
        NotificationReason::ReviewRequested => "your review was requested",
    }
}

const fn kind_label(kind: IssueKind) -> &'static str {
    match kind {
        IssueKind::Issue => "issue",
        IssueKind::Pull => "pull request",
    }
}

/// The URL segment of a thread (`/owner/repo/{segment}/N`), also used in message ids.
const fn thread_segment(kind: IssueKind) -> &'static str {
    match kind {
        IssueKind::Issue => "issues",
        IssueKind::Pull => "pulls",
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// `text` trimmed and cut to [`MAX_EXCERPT_CHARS`] characters.
fn excerpt_of(text: &str) -> String {
    cut(text.trim(), MAX_EXCERPT_CHARS)
}

/// `text` on one line (runs of whitespace become one space), cut to
/// [`MAX_DIGEST_EXCERPT_CHARS`] characters.
fn one_line_excerpt(text: &str) -> String {
    cut(&text.split_whitespace().collect::<Vec<_>>().join(" "), MAX_DIGEST_EXCERPT_CHARS)
}

fn cut(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", text[..end].trim_end()),
        None => text.to_owned(),
    }
}

/// The right-hand side of message ids: the public URL's host, without port or user info.
fn message_id_host(base_url: &str) -> String {
    let authority = base_url.split_once("://").map_or(base_url, |(_, rest)| rest);
    let authority = authority.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = if authority.starts_with('[') {
        // An IPv6 literal keeps its brackets: `[::1]` is a valid domain literal.
        authority.split_inclusive(']').next().unwrap_or(authority)
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    if host.is_empty() { "localhost".to_owned() } else { host.to_ascii_lowercase() }
}

type LinkMac = Hmac<Sha256>;

fn link_mac(key: &[u8], account: AccountId) -> LinkMac {
    // HMAC accepts keys of any length; this cannot fail.
    let mut mac = <LinkMac as KeyInit>::new_from_slice(key).unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
    mac.update(LINK_PURPOSE.as_bytes());
    mac.update(b"\0");
    mac.update(account.0.to_string().as_bytes());
    mac
}

/// `{account id}.{lower-case hex MAC}`.
fn sign_link(key: &[u8], account: AccountId) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let tag = link_mac(key, account).finalize().into_bytes();
    let mut token = format!("{}.", account.0);
    for byte in tag {
        token.push(char::from(DIGITS[usize::from(byte >> 4)]));
        token.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    token
}

/// The account a token was signed for, if the MAC matches (compared in constant time).
fn verify_link(key: &[u8], token: &str) -> Option<AccountId> {
    let (id, hex) = token.split_once('.')?;
    if id.is_empty() || id.len() > 19 || !id.bytes().all(|b| b.is_ascii_digit()) || id.starts_with('0') {
        return None;
    }
    let account = AccountId(id.parse().ok()?);
    if hex.len() != 64 {
        return None;
    }
    let tag: Vec<u8> = (0..32).map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()).collect::<Option<_>>()?;
    link_mac(key, account).verify_slice(&tag).ok().map(|()| account)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn links_verify_only_unchanged() {
        let token = sign_link(KEY, AccountId(42));
        assert!(token.starts_with("42."));
        assert_eq!(token.len(), 3 + 64);
        assert_eq!(verify_link(KEY, &token), Some(AccountId(42)));

        assert_eq!(verify_link(b"another key", &token), None, "another server's key");
        let other_account = token.replacen("42.", "43.", 1);
        assert_eq!(verify_link(KEY, &other_account), None, "the MAC binds the account");
        let mut flipped = token.clone();
        let last = flipped.pop().unwrap();
        flipped.push(if last == '0' { '1' } else { '0' });
        assert_eq!(verify_link(KEY, &flipped), None, "a changed MAC");
        for malformed in ["", "42", "42.", ".abc", "042.00", "x.y", &token[..token.len() - 2], &format!("{token}00"), "-1.00"] {
            assert_eq!(verify_link(KEY, malformed), None, "{malformed:?}");
        }
        let upper = token.to_uppercase();
        assert_eq!(verify_link(KEY, &upper), Some(AccountId(42)), "hex case does not matter");
    }

    #[test]
    fn preferences_decide_by_reason() {
        for reason in NotificationReason::ALL {
            assert!(!wants(EmailPreference::Off, reason));
            assert!(wants(EmailPreference::All, reason));
            assert_eq!(wants(EmailPreference::Participating, reason), reason != NotificationReason::Manual, "{reason:?}");
        }
    }

    #[test]
    fn message_ids_use_the_bare_host() {
        assert_eq!(message_id_host("https://git.example.com"), "git.example.com");
        assert_eq!(message_id_host("http://127.0.0.1:3000"), "127.0.0.1");
        assert_eq!(message_id_host("https://Git.Example.com:8443/prefix"), "git.example.com");
        assert_eq!(message_id_host("http://[::1]:3000"), "[::1]");
        assert_eq!(message_id_host("https://user@host.example"), "host.example");
        assert_eq!(message_id_host(""), "localhost");
    }

    #[test]
    fn excerpts_are_short() {
        assert_eq!(excerpt_of("  hello \n"), "hello");
        let long = "é".repeat(MAX_EXCERPT_CHARS + 5);
        let excerpt = excerpt_of(&long);
        assert_eq!(excerpt.chars().count(), MAX_EXCERPT_CHARS + 1);
        assert!(excerpt.ends_with('…'));
        assert_eq!(one_line_excerpt("  Same\n\nhere,\tplease "), "Same here, please");
        let excerpt = one_line_excerpt(&"word ".repeat(100));
        assert_eq!(excerpt.chars().count(), MAX_DIGEST_EXCERPT_CHARS);
        assert!(excerpt.ends_with("word…"), "cut at a word, trailing space dropped: {excerpt:?}");
        assert_eq!(capitalised("issue #3 was closed."), "Issue #3 was closed.");
        assert_eq!(capitalised(""), "");
    }
}
