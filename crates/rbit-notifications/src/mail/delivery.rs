//! Mailing one activity to the recipients the inbox recorded.
//!
//! Each recipient is mailed only if they have a verified address, want it (their
//! preference, or the configured default; `participating` leaves out threads only
//! subscribed to by hand), are not the actor, can still read the repository at send
//! time, were not mailed about this activity before (one dedup claim per recipient,
//! thread and activity) and, for immediate emails, are within their throttle budget.
//! Digest users get the same checks and claim, but their notification is queued.
use super::{
    activity::MailActivity,
    policy::{MailPreference, one_line_excerpt, wants},
    port::MailOutbox,
    records::NewDigestItem,
};
use crate::{NOTIFICATION_BATCH_SIZE, NotificationReason};
use rbit_collaboration::threads::IssueId;
use rbit_contracts::{AccountId, RepoId, Timestamp};
use std::collections::{BTreeMap, HashMap};

/// The thread, actor and time one activity's emails are about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailScope {
    pub repo: RepoId,
    pub issue: IssueId,
    pub actor: Option<AccountId>,
    pub now: Timestamp,
    /// The preference of users who never chose.
    pub default: MailPreference,
}

/// Mails (or queues for digests) `activity` to `recipients`, each with the reason the
/// inbox recorded. Authorization is refreshed for each batch and never reused from the
/// inbox; a batch is rendered completely before any of its dedup keys is claimed.
pub async fn deliver<O: MailOutbox>(
    outbox: &O,
    scope: MailScope,
    activity: &MailActivity,
    recipients: &[(AccountId, NotificationReason)],
) -> Result<(), O::Error> {
    let mut unique = BTreeMap::<AccountId, NotificationReason>::new();
    for &(user, reason) in recipients.iter().filter(|(user, _)| Some(*user) != scope.actor) {
        unique.entry(user).and_modify(|current| *current = (*current).max(reason)).or_insert(reason);
    }
    let recipients: Vec<_> = unique.into_iter().collect();
    for batch in recipients.chunks(NOTIFICATION_BATCH_SIZE) {
        // Ownership or visibility can change while earlier batches are delivered.
        let Some(repo) = outbox.repository(scope.repo).await? else { break };
        let ids: Vec<_> = batch.iter().map(|(user, _)| *user).collect();
        // This fresh read includes suspension, verification and preference.
        let users = outbox.recipients(&ids).await?;
        let reasons: HashMap<_, _> = batch.iter().copied().collect();
        let users: Vec<_> = users
            .into_iter()
            .filter_map(|recipient| {
                let reason = *reasons.get(&recipient.account)?;
                wants(recipient.preference.unwrap_or(scope.default), reason).then_some((recipient, reason))
            })
            .collect();
        let readers = outbox.readers(&repo, &users.iter().map(|(user, _)| user).collect::<Vec<_>>()).await?;
        let users: Vec<_> = users.into_iter().filter(|(user, _)| readers.contains(&user.account)).collect();
        // Rendering may fail; finish the bounded batch before claiming any of its keys.
        let mut rendered = Vec::with_capacity(users.len());
        let mut digest = Vec::new();
        for (user, reason) in users {
            if user.cadence.is_digest() {
                digest.push((user.account, reason));
            } else {
                let mail = outbox.render(&repo, &user, activity, reason).await?;
                rendered.push((user, mail));
            }
        }
        let ids: Vec<_> = rendered.iter().map(|(user, _)| user.account).chain(digest.iter().map(|(user, _)| *user)).collect();
        // One claim for both: switching between immediate and digest delivery never
        // sends an activity twice.
        let claimed = outbox.claim(&ids, scope.issue, &activity.key, scope.now).await?;
        let queued: Vec<_> = digest
            .into_iter()
            .filter(|(user, _)| claimed.contains(user))
            .map(|(account, reason)| NewDigestItem {
                account,
                repo: scope.repo,
                issue: scope.issue,
                reason,
                headline: activity.headline.clone(),
                excerpt: one_line_excerpt(&activity.excerpt),
                anchor: activity.anchor.clone(),
                created_at: scope.now,
            })
            .collect();
        outbox.queue_digest(&queued).await?;
        for (user, mail) in rendered.into_iter().filter(|(user, _)| claimed.contains(&user.account)) {
            outbox.send(&user, mail);
        }
    }
    Ok(())
}
