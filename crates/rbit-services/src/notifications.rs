//! In-app notifications: participation-based threads per (user, issue), the unread
//! indicator in the header and the `/notifications` inbox.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Hooks (`docs/design/milestone-2b.md` §3): the `Page` extractor calls
//! [`NotificationService::has_unread`] for signed-in users on every page; I's issue page
//! and P's pull request page call [`NotificationService::mark_thread_read`].
//!
//! A background subscriber handles `Issue`, `IssueComment` and `PullRequestReview`
//! events. Participants (author, commenters, assignees, mentioned users, requested
//! reviewers, reviewers) are subscribed automatically; recipients are the subscribers
//! plus the people the event names (new mentions, the assignee, the requested reviewer),
//! **minus the actor, suspended users and anyone who cannot read the repository at that
//! moment**. Mentions count only in Markdown text (never in code), at most 50 per text,
//! and only for users not taking part yet. Everything is disabled by
//! `[issues] notifications = false`.
//!
//! Milestone 3 workstream N: the recipients written to the inbox are handed on to
//! [`NotificationEmailService`], which also mails those who want it. The background work
//! here also sends that service's daily and weekly digests.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_auth::{Grants, repo_access};
use rbit_core::{
    AccountId, IssueId, RepoId, Timestamp, User,
    issue::{CommentAction, IssueAction, ReviewAction},
    references::scan_markdown,
    repository::OwnedRepository,
};
pub use rbit_storage::sqlite::notifications::{Notification, NotificationReason, Subscription};
use rbit_storage::sqlite::{
    collaborators::ViewerGrants,
    notifications::{NOTIFICATION_BATCH_SIZE, NotificationRecipient},
};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{DomainEvent, NotificationEmailService, ServiceContext, ServiceError};

pub use rbit_notifications::{InboxPage, NOTIFICATIONS_PER_PAGE};
mod inbox;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const PRUNE_INTERVAL: Duration = Duration::from_hours(24);
/// How often due email digests are looked for; a digest goes out at most this long after
/// its hour.
const DIGEST_INTERVAL: Duration = Duration::from_mins(5);
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

pub(crate) fn can_read_with_grants(user: &User, repo: &OwnedRepository, stored: Option<&ViewerGrants>) -> bool {
    let grants = stored.map_or_else(Grants::default, |stored| Grants {
        org_role: stored.org_role,
        org_base: stored.org_base.permission(),
        team: stored.team,
        collaborator: stored.collaborator,
    });
    repo_access(Some(user), repo, &grants).can_read()
}

#[derive(Debug)]
struct Background {
    started: AtomicBool,
    shutdown: CancellationToken,
    tasks: TaskTracker,
}

#[derive(Debug, Clone)]
pub struct NotificationService {
    ctx: ServiceContext,
    background: Arc<Background>,
    /// Milestone 3 (N): also mails what the fan-out writes to the inbox.
    email: Option<NotificationEmailService>,
}

impl NotificationService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let background =
            Arc::new(Background { started: AtomicBool::new(false), shutdown: CancellationToken::new(), tasks: TaskTracker::new() });
        Self { ctx, background, email: None }
    }

    /// The same service, also sending notifications by email through `email` (tests pass
    /// one with a memory transport).
    #[must_use]
    pub fn with_email(&self, email: NotificationEmailService) -> Self {
        Self { email: Some(email), ..self.clone() }
    }

    /// Whether notifications are on (`[issues] notifications`).
    pub fn enabled(&self) -> bool {
        self.ctx.config.issues.notifications
    }

    // ------------------------------------------------------------------ fan-out

    /// Reacts to one event: what the background subscriber does, callable directly (tests).
    pub async fn handle_event(&self, event: &DomainEvent) -> Result<(), ServiceError> {
        if !self.enabled() {
            return Ok(());
        }
        let now = Timestamp::now();
        let store = self.ctx.database.notifications();
        // Who to notify, with the reason the event itself gives them (if any).
        let mut direct: BTreeMap<AccountId, NotificationReason> = BTreeMap::new();
        let (repo, issue, actor, include_subscribers) = match event {
            DomainEvent::Issue { repo, issue, actor, action, .. } => {
                let include = match action {
                    IssueAction::Opened => {
                        let Some(row) = self.ctx.database.issues().find(*issue).await? else { return Ok(()) };
                        if let Some(author) = row.author {
                            store.participate(*issue, author, NotificationReason::Author, now).await?;
                        }
                        for (_, account) in self.ctx.database.issue_meta().assignees_of(&[*issue]).await? {
                            direct.insert(account.id, NotificationReason::Assigned);
                        }
                        self.new_mentions(*issue, row.body.as_str(), &mut direct).await?;
                        false
                    }
                    IssueAction::Edited { previous_body: Some(_), .. } => {
                        let Some(row) = self.ctx.database.issues().find(*issue).await? else { return Ok(()) };
                        self.new_mentions(*issue, row.body.as_str(), &mut direct).await?;
                        false
                    }
                    IssueAction::Assigned { assignee } => {
                        direct.insert(*assignee, NotificationReason::Assigned);
                        false
                    }
                    IssueAction::ReviewRequested { reviewer } => {
                        direct.insert(*reviewer, NotificationReason::ReviewRequested);
                        false
                    }
                    IssueAction::Closed { .. } | IssueAction::Reopened => true,
                    _ => return Ok(()),
                };
                (*repo, *issue, *actor, include)
            }
            DomainEvent::IssueComment { repo, issue, comment, actor, action, .. } => {
                let include = match action {
                    CommentAction::Created => {
                        // rbit Actions (no actor) is nobody to subscribe.
                        if let Some(actor) = actor {
                            store.participate(*issue, *actor, NotificationReason::Comment, now).await?;
                        }
                        true
                    }
                    CommentAction::Edited { .. } => false,
                    CommentAction::Deleted { .. } => return Ok(()),
                };
                if let Some(comment) = self.ctx.database.issues().find_comment(*issue, *comment).await? {
                    self.new_mentions(*issue, comment.body.as_str(), &mut direct).await?;
                }
                (*repo, *issue, *actor, include)
            }
            DomainEvent::PullRequestReview { repo, pull, actor, action: ReviewAction::Submitted, .. } => {
                store.participate(*pull, *actor, NotificationReason::Reviewed, now).await?;
                (*repo, *pull, Some(*actor), true)
            }
            _ => return Ok(()),
        };

        let mut recipients: BTreeMap<AccountId, NotificationReason> = BTreeMap::new();
        if include_subscribers {
            recipients.extend(store.subscribers(issue).await?);
        }
        for (user, reason) in &direct {
            let entry = recipients.entry(*user).or_insert(*reason);
            *entry = (*entry).max(*reason);
        }
        if let Some(actor) = actor {
            // Assigning oneself still makes one a participant; it never notifies.
            if let Some(reason) = direct.get(&actor).filter(|reason| **reason != NotificationReason::Mention) {
                store.participate(issue, actor, *reason, now).await?;
            }
            recipients.remove(&actor);
        }
        if recipients.is_empty() && direct.is_empty() {
            return Ok(());
        }
        let recipients: Vec<_> = recipients.into_iter().collect();
        let notified = self.notify_recipients(repo, issue, actor, now, &recipients, &direct).await?;
        // The inbox is written; email is best effort and never fails the fan-out.
        if let Some(email) = &self.email
            && let Err(error) = email.deliver(event, repo, issue, actor, &notified).await
        {
            tracing::warn!(%error, issue = issue.0, "cannot send notification emails");
        }
        Ok(())
    }

    async fn notify_recipients(
        &self,
        repo: RepoId,
        issue: IssueId,
        actor: Option<AccountId>,
        at: Timestamp,
        recipients: &[(AccountId, NotificationReason)],
        direct: &BTreeMap<AccountId, NotificationReason>,
    ) -> Result<Vec<(AccountId, NotificationReason)>, ServiceError> {
        let database = &self.ctx.database;
        let mut notified = Vec::with_capacity(recipients.len());
        for batch in recipients.chunks(NOTIFICATION_BATCH_SIZE) {
            let Some(owned) = database.admin().find_repository(repo).await? else { break };
            let ids: Vec<_> = batch.iter().map(|(id, _)| *id).collect();
            // Refresh ownership, visibility, active users and grants for each batch.
            // Authorization from an earlier batch or email delivery is not reused.
            let users = database.accounts().find_active_users(&ids).await?;
            let grant_ids: Vec<_> = users.iter().filter(|user| !user.is_admin && user.id() != owned.repo.owner_id).map(User::id).collect();
            let grants = database.repositories().grants_for(repo, owned.repo.owner_id, &grant_ids).await?;
            let reasons: HashMap<_, _> = batch.iter().copied().collect();
            let authorized: Vec<_> = users
                .iter()
                .filter(|user| can_read_with_grants(user, &owned, grants.get(&user.id())))
                .filter_map(|user| {
                    reasons.get(&user.id()).map(|reason| NotificationRecipient {
                        user: user.id(),
                        reason: *reason,
                        participation: direct.get(&user.id()).copied(),
                    })
                })
                .collect();
            notified.extend(database.notifications().notify_many(issue, repo, actor, at, &authorized).await?);
        }
        Ok(notified)
    }

    /// Adds the users mentioned in `text` who do not take part in `issue` yet.
    async fn new_mentions(
        &self,
        issue: IssueId,
        text: &str,
        into: &mut BTreeMap<AccountId, NotificationReason>,
    ) -> Result<(), ServiceError> {
        let mentions = scan_markdown(text).mentions;
        if mentions.is_empty() {
            return Ok(());
        }
        let accounts = self.ctx.database.issue_meta().accounts_named(&mentions).await?;
        let ids: Vec<AccountId> = accounts.iter().filter(|a| !a.is_organization()).map(|a| a.id).collect();
        let known = self.ctx.database.notifications().known_participants(issue, &ids).await?;
        for id in ids.into_iter().filter(|id| !known.contains(id)) {
            let entry = into.entry(id).or_insert(NotificationReason::Mention);
            *entry = (*entry).max(NotificationReason::Mention);
        }
        Ok(())
    }

    /// Deletes read notifications older than `notification_retention_days`.
    pub async fn prune(&self) -> Result<u64, ServiceError> {
        let days = i64::from(self.ctx.config.issues.notification_retention_days);
        let cutoff = Timestamp(Timestamp::now().0.saturating_sub(days.saturating_mul(SECONDS_PER_DAY)));
        if let Some(email) = &self.email {
            email.prune(cutoff).await?;
        }
        Ok(self.ctx.database.notifications().prune_read(cutoff).await?)
    }

    // ------------------------------------------------------------------ background work

    /// Starts the notification fan-out subscriber, the daily pruning and, with email, the
    /// digest schedule. Called once from
    /// [`crate::Services::spawn_background`]; does nothing when notifications are off.
    pub fn spawn_background(&self) {
        if self.background.started.swap(true, Ordering::SeqCst) || !self.enabled() {
            return;
        }
        let events = self.ctx.events.subscribe();
        let service = self.clone();
        self.background.tasks.spawn(async move { service.listen(events).await });
        let service = self.clone();
        self.background.tasks.spawn(async move { service.prune_periodically().await });
        if let Some(email) = self.email.clone() {
            let shutdown = self.background.shutdown.clone();
            self.background.tasks.spawn(async move { send_digests_periodically(email, shutdown).await });
        }
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.background.shutdown.cancel();
        self.background.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.background.tasks.wait()).await.is_err() {
            tracing::warn!("notifications did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn listen(self, mut events: tokio::sync::broadcast::Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.background.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(event) => {
                    if let Err(error) = self.handle_event(&event).await {
                        tracing::warn!(%error, ?event, "cannot deliver notifications");
                    }
                }
                Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "notification subscriber fell behind"),
                Err(RecvError::Closed) => break,
            }
        }
    }

    async fn prune_periodically(self) {
        loop {
            match self.prune().await {
                Ok(0) => {}
                Ok(pruned) => tracing::info!(pruned, "pruned read notifications"),
                Err(error) => tracing::warn!(%error, "cannot prune notifications"),
            }
            tokio::select! {
                () = self.background.shutdown.cancelled() => break,
                () = tokio::time::sleep(PRUNE_INTERVAL) => {}
            }
        }
    }
}

/// Sends due digests on start-up (catching up after downtime) and then every
/// [`DIGEST_INTERVAL`]. A run interrupted by shutdown is safe: claimed digests are never
/// sent twice.
async fn send_digests_periodically(email: NotificationEmailService, shutdown: CancellationToken) {
    loop {
        let run = tokio::select! {
            () = shutdown.cancelled() => break,
            run = email.send_digests() => run,
        };
        match run {
            Ok(run) if run.sent + run.failed + run.empty > 0 => {
                tracing::info!(sent = run.sent, failed = run.failed, empty = run.empty, "notification digests processed");
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "cannot send notification digests"),
        }
        tokio::select! {
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(DIGEST_INTERVAL) => {}
        }
    }
}
