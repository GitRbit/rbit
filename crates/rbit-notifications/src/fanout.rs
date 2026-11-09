//! Who hears about an activity, and why; stores record it and access decides readers.
//!
//! Participants (author, commenters, assignees, mentioned users, requested reviewers,
//! reviewers) are subscribed automatically. Recipients are the subscribers (for
//! activities everyone follows) plus the people the activity names, each with their
//! strongest reason, **minus the actor, suspended users and anyone who cannot read the
//! repository at that moment**. Mentions (found in Markdown text, never in code, at most
//! 50 per text, by the shared scanner) count only for users not taking part yet, never
//! organizations.
use crate::{
    Activity, ActivityEvent, NotificationReason, NotificationRecipient,
    port::{ActivitySource, DeliveryRepository, RecipientAccess},
};
use rbit_collaboration::threads::IssueId;
use rbit_contracts::{AccountId, Timestamp};
use std::collections::{BTreeMap, HashMap};

/// Maximum recipients authorized and recorded together, keeping `SQLite`'s writer
/// available between batches even for threads with many subscribers.
pub const NOTIFICATION_BATCH_SIZE: usize = 100;

/// The recipients an activity reached, each with the reason recorded for them.
pub type Notified = Vec<(AccountId, NotificationReason)>;

/// Borrowed fan-out over one store, one content source and one access owner, all of
/// which share the store's error type.
pub struct FanoutApplication<'a, R, S, A> {
    repository: &'a R,
    source: &'a S,
    access: &'a A,
}

impl<'a, R, S, A> FanoutApplication<'a, R, S, A>
where
    R: DeliveryRepository,
    S: ActivitySource<Error = R::Error>,
    A: RecipientAccess<Error = R::Error>,
{
    pub fn new(repository: &'a R, source: &'a S, access: &'a A) -> Self {
        Self { repository, source, access }
    }

    /// Records `event` at `now`. `None` when nobody is named or subscribed to hear of
    /// it (or the thread is gone); otherwise everyone notified, possibly nobody when
    /// the only named user is the actor.
    pub async fn deliver(&self, event: ActivityEvent, now: Timestamp) -> Result<Option<Notified>, R::Error> {
        let ActivityEvent { issue, actor, activity, .. } = event;
        // Who to notify, with the reason the activity itself gives them (if any).
        let mut direct = BTreeMap::new();
        let include_subscribers = match activity {
            Activity::Opened => {
                let Some(thread) = self.source.thread(issue).await? else { return Ok(None) };
                if let Some(author) = thread.author {
                    self.repository.participate(issue, author, NotificationReason::Author, now).await?;
                }
                for account in self.source.assignees(issue).await? {
                    direct.insert(account, NotificationReason::Assigned);
                }
                self.new_mentions(issue, thread.body.as_str(), &mut direct).await?;
                false
            }
            Activity::BodyEdited => {
                let Some(thread) = self.source.thread(issue).await? else { return Ok(None) };
                self.new_mentions(issue, thread.body.as_str(), &mut direct).await?;
                false
            }
            Activity::Assigned(assignee) => {
                direct.insert(assignee, NotificationReason::Assigned);
                false
            }
            Activity::ReviewRequested(reviewer) => {
                direct.insert(reviewer, NotificationReason::ReviewRequested);
                false
            }
            Activity::StateChanged => true,
            Activity::CommentCreated(comment) | Activity::CommentEdited(comment) => {
                let created = matches!(activity, Activity::CommentCreated(_));
                // An automated task (no actor) is nobody to subscribe.
                if created && let Some(actor) = actor {
                    self.repository.participate(issue, actor, NotificationReason::Comment, now).await?;
                }
                if let Some(body) = self.source.comment_body(issue, comment).await? {
                    self.new_mentions(issue, body.as_str(), &mut direct).await?;
                }
                created
            }
            Activity::ReviewSubmitted => {
                if let Some(actor) = actor {
                    self.repository.participate(issue, actor, NotificationReason::Reviewed, now).await?;
                }
                true
            }
        };

        let mut recipients = BTreeMap::new();
        if include_subscribers {
            recipients.extend(self.repository.subscribers(issue).await?);
        }
        for (user, reason) in &direct {
            let entry = recipients.entry(*user).or_insert(*reason);
            *entry = (*entry).max(*reason);
        }
        if let Some(actor) = actor {
            // Assigning oneself still makes one a participant; it never notifies.
            if let Some(reason) = direct.get(&actor).filter(|reason| **reason != NotificationReason::Mention) {
                self.repository.participate(issue, actor, *reason, now).await?;
            }
            recipients.remove(&actor);
        }
        if recipients.is_empty() && direct.is_empty() {
            return Ok(None);
        }
        let recipients: Vec<_> = recipients.into_iter().collect();
        self.notify(event, now, &recipients, &direct).await.map(Some)
    }

    /// Authorization is refreshed for each batch; an earlier batch is never reused.
    async fn notify(
        &self,
        event: ActivityEvent,
        at: Timestamp,
        recipients: &[(AccountId, NotificationReason)],
        direct: &BTreeMap<AccountId, NotificationReason>,
    ) -> Result<Notified, R::Error> {
        let mut notified = Vec::with_capacity(recipients.len());
        for batch in recipients.chunks(NOTIFICATION_BATCH_SIZE) {
            let ids: Vec<_> = batch.iter().map(|(id, _)| *id).collect();
            let Some(readers) = self.access.readers(event.repo, &ids).await? else { break };
            let reasons: HashMap<_, _> = batch.iter().copied().collect();
            let authorized: Vec<_> = readers
                .into_iter()
                .filter_map(|user| {
                    reasons.get(&user).map(|reason| NotificationRecipient {
                        user,
                        reason: *reason,
                        participation: direct.get(&user).copied(),
                    })
                })
                .collect();
            notified.extend(self.repository.notify_many(event.issue, event.repo, event.actor, at, &authorized).await?);
        }
        Ok(notified)
    }

    /// Adds the users mentioned in `text` who do not take part in `issue` yet.
    async fn new_mentions(&self, issue: IssueId, text: &str, into: &mut BTreeMap<AccountId, NotificationReason>) -> Result<(), R::Error> {
        let accounts = self.source.mentioned(text).await?;
        if accounts.is_empty() {
            return Ok(());
        }
        let ids: Vec<AccountId> = accounts.iter().filter(|account| !account.organization).map(|account| account.id).collect();
        let known = self.repository.known_participants(issue, &ids).await?;
        for id in ids.into_iter().filter(|id| !known.contains(id)) {
            let entry = into.entry(id).or_insert(NotificationReason::Mention);
            *entry = (*entry).max(NotificationReason::Mention);
        }
        Ok(())
    }
}

// Payload Debug bounds would unnecessarily expose stores, sources or access owners.
impl<R, S, A> std::fmt::Debug for FanoutApplication<'_, R, S, A> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("FanoutApplication").finish_non_exhaustive()
    }
}
