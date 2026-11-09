//! Runtime composition of feature-owned email delivery: concrete stores, the shared
//! access predicate, templates, throttling and the mail transport answer its ports.
use super::NotificationEmailService;
use crate::{ServiceError, ThrottleScope, notifications::can_read_with_grants};
use rbit_core::{
    AccountId, Issue, IssueId, IssueKind, RepoId, Timestamp, User,
    config::{EmailDelivery, EmailPreference},
    repository::OwnedRepository,
};
use rbit_mail::{
    Mail,
    messages::{DigestEntry, DigestNotification, DigestRepository, DigestThread, MailHeaders, ThreadNotification},
};
use rbit_notifications::{
    NotificationReason,
    mail::{
        activity::MailActivity,
        digest::{DigestSection, prune_cutoff},
        policy::{
            Cadence, MailPreference, SETTINGS_PATH, digest_message_id, message_id_host, reason_clause, thread_link, thread_message_ids,
            thread_path,
        },
        port::{DigestOutbox, MailOutbox},
        records::{Claim, ClaimedDigest, DigestItem, DigestState, MailRecipient, NewDigestItem, PendingDigest},
    },
};
use rbit_storage::{Database, sqlite::notification_email::EmailRecipient};
use std::collections::{HashMap, HashSet};

pub(super) const fn preference(preference: EmailPreference) -> MailPreference {
    match preference {
        EmailPreference::Off => MailPreference::Off,
        EmailPreference::Participating => MailPreference::Participating,
        EmailPreference::All => MailPreference::All,
    }
}

pub(super) const fn cadence(delivery: EmailDelivery) -> Cadence {
    match delivery {
        EmailDelivery::Immediate => Cadence::Immediate,
        EmailDelivery::Daily => Cadence::Daily,
        EmailDelivery::Weekly => Cadence::Weekly,
    }
}

fn recipient(row: EmailRecipient) -> MailRecipient<User> {
    MailRecipient { account: row.user.id(), preference: row.preference.map(preference), cadence: cadence(row.delivery), user: row.user }
}

/// Deletes outbox rows before `cutoff` and digests before the feature's digest cutoff.
pub(super) async fn prune(database: &Database, cutoff: Timestamp, now: Timestamp) -> Result<u64, ServiceError> {
    let outbox = database.notification_email().prune(cutoff).await?;
    let digests = database.email_digests().prune(prune_cutoff(cutoff, now)).await?;
    Ok(outbox + digests)
}

/// Whether `user` may read `owned` with the grants stored for them.
async fn may_read(database: &Database, user: &User, owned: &OwnedRepository) -> Result<bool, ServiceError> {
    let grants = if user.is_admin || user.id() == owned.repo.owner_id {
        HashMap::new()
    } else {
        database.repositories().grants_for(owned.repo.id, owned.repo.owner_id, &[user.id()]).await?
    };
    Ok(can_read_with_grants(user, owned, grants.get(&user.id())))
}

/// One activity's emails, bound to its thread.
pub(super) struct ThreadOutbox<'a> {
    service: &'a NotificationEmailService,
    thread: &'a Issue,
}

impl<'a> ThreadOutbox<'a> {
    pub(super) const fn new(service: &'a NotificationEmailService, thread: &'a Issue) -> Self {
        Self { service, thread }
    }
}

impl MailOutbox for ThreadOutbox<'_> {
    type Error = ServiceError;
    type Repository = OwnedRepository;
    type User = User;
    type Mail = Mail;

    async fn repository(&self, repo: RepoId) -> Result<Option<OwnedRepository>, ServiceError> {
        Ok(self.service.ctx.database.admin().find_repository(repo).await?)
    }

    async fn recipients(&self, accounts: &[AccountId]) -> Result<Vec<MailRecipient<User>>, ServiceError> {
        Ok(self.service.ctx.database.notification_email().recipients(accounts).await?.into_iter().map(recipient).collect())
    }

    async fn readers(&self, owned: &OwnedRepository, users: &[&MailRecipient<User>]) -> Result<HashSet<AccountId>, ServiceError> {
        let grant_ids: Vec<_> =
            users.iter().map(|user| &user.user).filter(|user| !user.is_admin && user.id() != owned.repo.owner_id).map(User::id).collect();
        let grants = self.service.ctx.database.repositories().grants_for(owned.repo.id, owned.repo.owner_id, &grant_ids).await?;
        Ok(users
            .iter()
            .filter(|user| can_read_with_grants(&user.user, owned, grants.get(&user.account)))
            .map(|user| user.account)
            .collect())
    }

    async fn render(
        &self,
        owned: &OwnedRepository,
        user: &MailRecipient<User>,
        activity: &MailActivity,
        reason: NotificationReason,
    ) -> Result<Mail, ServiceError> {
        let thread = self.thread;
        let base_url = self.service.ctx.config.server.base_url();
        let full_name = owned.full_name();
        let path = thread_path(&full_name, thread.kind, thread.number.get());
        let link = thread_link(base_url, &path, activity.anchor.as_deref());
        let unsubscribe_link = self.service.unsubscribe_link(user.account).await?;
        let message = ThreadNotification {
            site_name: self.service.instance.current().site_name,
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
        let (message_id, in_reply_to) = thread_message_ids(&path, &message_id_host(base_url), &activity.key, activity.opening);
        let headers = MailHeaders { message_id: Some(message_id), in_reply_to, one_click_unsubscribe: Some(unsubscribe_link) };
        Ok(Mail::render(user.user.email.clone(), &message).map_err(ServiceError::internal)?.with_headers(headers))
    }

    async fn claim(&self, accounts: &[AccountId], issue: IssueId, key: &str, now: Timestamp) -> Result<HashSet<AccountId>, ServiceError> {
        Ok(self.service.ctx.database.notification_email().claim_many(accounts, issue, key, now).await?)
    }

    async fn queue_digest(&self, items: &[NewDigestItem]) -> Result<(), ServiceError> {
        Ok(self.service.ctx.database.email_digests().queue(items).await?)
    }

    fn send(&self, user: &MailRecipient<User>, mail: Mail) {
        let user = &user.user;
        if let Err(error) = self.service.throttle.check(ThrottleScope::NotificationEmail, &user.email.as_str().to_lowercase()) {
            tracing::info!(%error, user = %user.name(), "notification email skipped: too many recently");
            return;
        }
        self.service.mail.send(mail);
    }
}

/// Digest runs over the concrete stores and the direct mail transport.
pub(super) struct DigestView<'a>(pub(super) &'a NotificationEmailService);

fn section(section: DigestSection) -> DigestRepository {
    let threads = section
        .threads
        .into_iter()
        .map(|thread| DigestThread {
            number: thread.number,
            title: thread.title,
            is_pull: thread.is_pull,
            link: thread.link,
            entries: thread.entries.into_iter().map(|(headline, excerpt, link)| DigestEntry { headline, excerpt, link }).collect(),
        })
        .collect();
    DigestRepository { name: section.name, link: section.link, threads }
}

impl DigestOutbox for DigestView<'_> {
    type Error = ServiceError;
    type User = User;
    type Mail = Mail;

    async fn candidates(&self, after: AccountId, limit: u32) -> Result<Vec<PendingDigest>, ServiceError> {
        let page = self.0.ctx.database.email_digests().candidates(after, limit).await?;
        Ok(page
            .into_iter()
            .map(|row| PendingDigest { account: row.account, cadence: cadence(row.delivery), digest_hour: row.digest_hour })
            .collect())
    }

    async fn claim(&self, account: AccountId, period: Timestamp, now: Timestamp) -> Result<Claim, ServiceError> {
        Ok(self.0.ctx.database.email_digests().claim(account, period, now).await?)
    }

    async fn finish(&self, digest: i64, state: DigestState, now: Timestamp) -> Result<(), ServiceError> {
        Ok(self.0.ctx.database.email_digests().finish(digest, state, now).await?)
    }

    async fn recipient(&self, account: AccountId) -> Result<Option<MailRecipient<User>>, ServiceError> {
        Ok(self.0.ctx.database.notification_email().recipients(&[account]).await?.into_iter().next().map(recipient))
    }

    async fn items(&self, digest: i64) -> Result<Vec<DigestItem>, ServiceError> {
        Ok(self.0.ctx.database.email_digests().items(digest).await?)
    }

    async fn can_read(&self, user: &MailRecipient<User>, repo: RepoId) -> Result<bool, ServiceError> {
        let database = &self.0.ctx.database;
        match database.admin().find_repository(repo).await? {
            Some(owned) => may_read(database, &user.user, &owned).await,
            None => Ok(false),
        }
    }

    async fn render(
        &self,
        user: &MailRecipient<User>,
        digest: &ClaimedDigest,
        weekly: bool,
        sections: Vec<DigestSection>,
    ) -> Result<Mail, ServiceError> {
        let base_url = self.0.ctx.config.server.base_url();
        let unsubscribe_link = self.0.unsubscribe_link(user.account).await?;
        let message = DigestNotification {
            site_name: self.0.instance.current().site_name,
            base_url: base_url.to_owned(),
            weekly,
            repositories: sections.into_iter().map(section).collect(),
            settings_link: format!("{base_url}{SETTINGS_PATH}"),
            unsubscribe_link: unsubscribe_link.clone(),
        };
        let headers = MailHeaders {
            message_id: Some(digest_message_id(user.account.0, digest.period.0, &message_id_host(base_url))),
            in_reply_to: None,
            one_click_unsubscribe: Some(unsubscribe_link),
        };
        Ok(Mail::render(user.user.email.clone(), &message).map_err(ServiceError::internal)?.with_headers(headers))
    }

    async fn deliver(&self, user: &MailRecipient<User>, mail: Mail, notifications: usize) -> bool {
        match self.0.mail.deliver(mail).await {
            Ok(()) => {
                tracing::info!(user = %user.user.name(), notifications, "notification digest sent");
                true
            }
            Err(error) => {
                tracing::warn!(%error, user = %user.user.name(), "notification digest not sent");
                false
            }
        }
    }

    fn build_failed(&self, account: AccountId, error: &ServiceError) {
        tracing::warn!(%error, account = account.0, "cannot build a notification digest");
    }

    fn record(&self, sent: bool) {
        let outcome = if sent { "sent" } else { "failed" };
        metrics::counter!("rbit_notification_digests_total", "outcome" => outcome).increment(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_settings_map_one_to_one() {
        assert_eq!(rbit_notifications::mail::policy::MAX_DIGEST_HOUR, rbit_core::config::MAX_DIGEST_HOUR);
        for value in EmailPreference::ALL {
            let mapped = preference(value);
            assert_eq!(format!("{mapped:?}").to_lowercase(), value.as_str());
        }
        for value in EmailDelivery::ALL {
            let mapped = cadence(value);
            assert_eq!(format!("{mapped:?}").to_lowercase(), value.as_str());
            assert_eq!(mapped.is_digest(), value.is_digest());
        }
    }
}
