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

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_auth::repo_access;
use rbit_core::{
    AccountId, IssueId, RepoId, Timestamp, User,
    issue::{CommentAction, IssueAction, ReviewAction},
    references::scan_markdown,
    repository::OwnedRepository,
};
pub use rbit_storage::sqlite::notifications::{Notification, NotificationReason, Subscription};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{DomainEvent, ServiceContext, ServiceError, repositories::viewer_grants};

/// Rows per inbox page.
pub const NOTIFICATIONS_PER_PAGE: u32 = 50;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const PRUNE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// One page of a user's inbox, rows of unreadable repositories already removed.
#[derive(Debug, Clone)]
pub struct InboxPage {
    pub notifications: Vec<Notification>,
    pub total: u64,
    /// 1-based.
    pub page: u32,
    pub per_page: u32,
}

impl InboxPage {
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.total.div_ceil(u64::from(self.per_page)).max(1)).unwrap_or(u32::MAX)
    }
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
}

impl NotificationService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let background =
            Arc::new(Background { started: AtomicBool::new(false), shutdown: CancellationToken::new(), tasks: TaskTracker::new() });
        Self { ctx, background }
    }

    /// Whether notifications are on (`[issues] notifications`).
    pub fn enabled(&self) -> bool {
        self.ctx.config.issues.notifications
    }

    /// Whether `user` has an unread notification (one indexed `EXISTS`).
    pub async fn has_unread(&self, user: AccountId) -> Result<bool, ServiceError> {
        if !self.enabled() {
            return Ok(false);
        }
        Ok(self.ctx.database.notifications().has_unread(user).await?)
    }

    /// Marks `user`'s notification about `issue` read, if any. Failures are logged, never
    /// returned: it runs as a side effect of viewing a page.
    pub async fn mark_thread_read(&self, user: AccountId, issue: IssueId) {
        if !self.enabled() {
            return;
        }
        if let Err(error) = self.ctx.database.notifications().mark_issue_read(user, issue).await {
            tracing::warn!(%error, user = user.0, issue = issue.0, "cannot mark a notification read");
        }
    }

    // ------------------------------------------------------------------ subscriptions

    /// `user`'s subscription to `issue`, if they have one.
    pub async fn subscription(&self, user: AccountId, issue: IssueId) -> Result<Option<Subscription>, ServiceError> {
        Ok(self.ctx.database.notifications().subscription(issue, user).await?)
    }

    /// Subscribes or unsubscribes `user` by hand (the caller checked they can read it).
    pub async fn set_subscribed(&self, user: AccountId, issue: IssueId, subscribed: bool) -> Result<(), ServiceError> {
        Ok(self.ctx.database.notifications().set_subscribed(issue, user, subscribed, Timestamp::now()).await?)
    }

    // ------------------------------------------------------------------ the inbox

    /// One page (1-based) of `user`'s inbox. Rows of repositories they can no longer read
    /// are left out and deleted.
    pub async fn inbox(&self, user: &User, unread_only: bool, page: u32) -> Result<InboxPage, ServiceError> {
        self.inbox_page(user, unread_only, page.max(1), NOTIFICATIONS_PER_PAGE).await
    }

    /// [`Self::inbox`] with a page size (the API's `per_page`).
    pub async fn inbox_page(&self, user: &User, unread_only: bool, page: u32, per_page: u32) -> Result<InboxPage, ServiceError> {
        let store = self.ctx.database.notifications();
        let per_page = per_page.max(1);
        let offset = u64::from(page.max(1) - 1) * u64::from(per_page);
        let rows = store.list(user.id(), unread_only, per_page, offset).await?;
        let mut readable: HashMap<RepoId, bool> = HashMap::new();
        let mut hidden = Vec::new();
        let mut notifications = Vec::with_capacity(rows.len());
        for row in rows {
            let can_read = if let Some(can_read) = readable.get(&row.repo) {
                *can_read
            } else {
                let can_read = match self.ctx.database.admin().find_repository(row.repo).await? {
                    Some(repo) => self.can_read(user, &repo).await?,
                    None => false,
                };
                readable.insert(row.repo, can_read);
                can_read
            };
            if can_read {
                notifications.push(row);
            } else {
                hidden.push(row.id);
            }
        }
        if !hidden.is_empty() {
            store.delete(user.id(), &hidden).await?;
        }
        let total = store.count(user.id(), unread_only).await?;
        Ok(InboxPage { notifications, total, page: page.max(1), per_page })
    }

    /// Notification `id` of `user`, while they can read its repository.
    pub async fn find(&self, user: &User, id: i64) -> Result<Notification, ServiceError> {
        let row = self.ctx.database.notifications().find(user.id(), id).await?.ok_or(ServiceError::NotFound)?;
        let repo = self.ctx.database.admin().find_repository(row.repo).await?.ok_or(ServiceError::NotFound)?;
        if !self.can_read(user, &repo).await? {
            self.ctx.database.notifications().delete(user.id(), &[id]).await?;
            return Err(ServiceError::NotFound);
        }
        Ok(row)
    }

    /// Marks notification `id` read; `NotFound` when it is not `user`'s.
    pub async fn mark_read(&self, user: &User, id: i64) -> Result<(), ServiceError> {
        if self.ctx.database.notifications().mark_read(user.id(), id).await? { Ok(()) } else { Err(ServiceError::NotFound) }
    }

    /// Marks everything up to now read.
    pub async fn mark_all_read(&self, user: &User) -> Result<u64, ServiceError> {
        Ok(self.ctx.database.notifications().mark_all_read(user.id(), Timestamp::now()).await?)
    }

    /// Unsubscribes `user` from the thread of notification `id` and marks it read.
    pub async fn unsubscribe(&self, user: &User, id: i64) -> Result<(), ServiceError> {
        let row = self.find(user, id).await?;
        self.set_subscribed(user.id(), row.issue, false).await?;
        self.ctx.database.notifications().mark_read(user.id(), id).await?;
        Ok(())
    }

    async fn can_read(&self, user: &User, repo: &OwnedRepository) -> Result<bool, ServiceError> {
        let grants = viewer_grants(&self.ctx.database.repositories(), Some(user), repo).await?;
        Ok(repo_access(Some(user), repo, &grants).can_read())
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
                        store.participate(*issue, *actor, NotificationReason::Comment, now).await?;
                        true
                    }
                    CommentAction::Edited { .. } => false,
                    CommentAction::Deleted { .. } => return Ok(()),
                };
                if let Some(comment) = self.ctx.database.issues().find_comment(*issue, *comment).await? {
                    self.new_mentions(*issue, comment.body.as_str(), &mut direct).await?;
                }
                (*repo, *issue, Some(*actor), include)
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
        let Some(owned) = self.ctx.database.admin().find_repository(repo).await? else { return Ok(()) };
        for (user_id, reason) in recipients {
            // Suspended users resolve to nothing; access is decided now, with grants.
            let Some(user) = self.ctx.database.accounts().find_active_user(user_id).await? else { continue };
            if !self.can_read(&user, &owned).await? {
                continue;
            }
            if let Some(direct) = direct.get(&user_id) {
                store.participate(issue, user_id, *direct, now).await?;
            }
            let subscription = store.subscription(issue, user_id).await?;
            let reason = subscription.filter(|s| s.subscribed).map_or(reason, |s| s.reason.max(reason));
            store.notify(user_id, issue, repo, reason, actor, now).await?;
        }
        Ok(())
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
        Ok(self.ctx.database.notifications().prune_read(cutoff).await?)
    }

    // ------------------------------------------------------------------ background work

    /// Starts the notification fan-out subscriber and the daily pruning. Called once from
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
