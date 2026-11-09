//! In-app notifications: participation-based threads per (user, issue), the unread
//! indicator in the header and the `/notifications` inbox.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Hooks (`docs/design/milestone-2b.md` §3): the `Page` extractor calls
//! [`NotificationService::has_unread`] for signed-in users on every page; I's issue page
//! and P's pull request page call [`NotificationService::mark_thread_read`].
//!
//! A background subscriber hands `Issue`, `IssueComment` and `PullRequestReview` events
//! to the feature-owned fan-out ([`rbit_notifications::fanout`]), which decides who
//! takes part and who hears about what; this service only maps events, composes the
//! stores and owns the background lifecycle. Everything is disabled by
//! `[issues] notifications = false`.
//!
//! Milestone 3 workstream N: the recipients written to the inbox are handed on to
//! [`NotificationEmailService`], which also mails those who want it. The background work
//! here also sends that service's daily and weekly digests.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_auth::{Grants, repo_access};
use rbit_core::{Timestamp, User, repository::OwnedRepository};
use rbit_notifications::retention;
pub use rbit_notifications::{InboxPage, NOTIFICATIONS_PER_PAGE};
pub use rbit_notifications::{Notification, NotificationReason, Subscription};
use rbit_storage::{NotificationInboxStore, sqlite::collaborators::ViewerGrants};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{DomainEvent, NotificationEmailService, ServiceContext, ServiceError};

mod fanout;
mod inbox;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const PRUNE_INTERVAL: Duration = Duration::from_hours(24);
/// How often due email digests are looked for; a digest goes out at most this long after
/// its hour.
const DIGEST_INTERVAL: Duration = Duration::from_mins(5);

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
        let Some(activity) = fanout::activity(event) else { return Ok(()) };
        let Some(notified) = fanout::deliver(&self.ctx.database, activity, Timestamp::now()).await? else { return Ok(()) };
        // The inbox is written; email is best effort and never fails the fan-out.
        if let Some(email) = &self.email
            && let Err(error) = email.deliver(event, activity.repo, activity.issue, activity.actor, &notified).await
        {
            tracing::warn!(%error, issue = activity.issue.0, "cannot send notification emails");
        }
        Ok(())
    }

    /// Deletes read notifications older than `notification_retention_days`.
    pub async fn prune(&self) -> Result<u64, ServiceError> {
        let cutoff = retention::cutoff(Timestamp::now(), self.ctx.config.issues.notification_retention_days);
        if let Some(email) = &self.email {
            email.prune(cutoff).await?;
        }
        Ok(retention::prune_read(&NotificationInboxStore::new(self.ctx.database.notifications()), cutoff).await?)
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
