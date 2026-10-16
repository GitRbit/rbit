//! Webhooks: configuration, event payloads, HMAC-signed deliveries with retries and
//! the delivery log. Deliveries are driven by `DomainEvent`s.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.
//!
//! * **Management.** Repository administrators manage a repository's hooks, organisation
//!   owners (and site administrators) an organisation's, which fire for every repository
//!   of the organisation. At most [`MAX_HOOKS_PER_OWNER`] per repository or organisation.
//!   Creating, changing and deleting a hook is recorded in the audit log. A hook's secret
//!   is write-only: it is stored as given (HMAC needs it; the documented exception to
//!   "secrets only as digests"), never returned, logged or copied into the delivery log.
//! * **Queueing.** A background subscriber turns [`DomainEvent`]s into one `pending`
//!   delivery per subscribed, active hook (one per changed ref for pushes) and does nothing
//!   else. Hooks get a `ping` when created.
//! * **Delivering.** A worker claims due deliveries, runs at most `[webhooks] concurrency`
//!   requests at once (see [`delivery`] for the client and its address guard), treats 2xx
//!   as success and retries everything else with exponential backoff and jitter until
//!   `max_attempts`, then gives up (`failed`). Targets the guard refuses fail at once.
//! * **Retention.** A daily task drops finished deliveries older than
//!   `delivery_retention_days`.
//!
//! The queue lives in SQLite, so a restart loses nothing: deliveries claimed when the
//! process stopped are returned to the queue at start-up. With `[webhooks] enabled =
//! false` hooks can still be configured, but no events are queued and nothing is sent.

pub mod delivery;
pub mod payloads;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rustit_core::{
    Account, AccountId, RepoId, Timestamp, User,
    repository::OwnedRepository,
    webhook::{Delivery, DeliveryId, Webhook, WebhookEvent, WebhookId, WebhookOwner, WebhookUrl},
};
pub use rustit_storage::sqlite::webhooks::SecretUpdate;
use rustit_storage::sqlite::webhooks::{AttemptOutcome, ClaimedDelivery, NewDelivery, NewWebhook, WebhookStore, WebhookUpdate};
use serde_json::Value;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, broadcast::error::RecvError};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    DomainEvent, OrgContext, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// Hooks per repository or organisation.
pub const MAX_HOOKS_PER_OWNER: i64 = 20;
/// Longest accepted secret.
const MAX_SECRET_LEN: usize = 256;
/// Most commits read from Git for one ref update (`total_commits` counts up to this).
const MAX_COUNTED_COMMITS: usize = 2_048;
/// How long [`WebhookService::shutdown`] waits for deliveries in flight.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// How often the worker looks for due retries when nothing wakes it.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// How often old deliveries are pruned.
const PRUNE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// What a hook is attached to, as loaded for the acting user.
#[derive(Debug, Clone, Copy)]
pub enum HookTarget<'a> {
    Repository(&'a RepoContext),
    Organization(&'a OrgContext),
}

impl HookTarget<'_> {
    pub fn owner(&self) -> WebhookOwner {
        match self {
            Self::Repository(repo) => WebhookOwner::Repository(repo.repo.repo.id),
            Self::Organization(org) => WebhookOwner::Organization(org.org.account.id),
        }
    }

    /// Repository administrators and organisation owners (and site administrators).
    pub fn can_manage(&self) -> bool {
        match self {
            Self::Repository(repo) => repo.access.can_admin(),
            Self::Organization(org) => org.can_manage,
        }
    }

    /// `owner/name` or the organisation's name, for audit entries.
    fn name(&self) -> String {
        match self {
            Self::Repository(repo) => repo.repo.full_name(),
            Self::Organization(org) => org.org.name().to_string(),
        }
    }

    /// The API URL of hook `id`.
    pub fn hook_api_url(&self, base_url: &str, id: WebhookId) -> String {
        match self {
            Self::Repository(repo) => format!("{base_url}/api/v1/repos/{}/hooks/{}", repo.repo.full_name(), id.0),
            Self::Organization(org) => format!("{base_url}/api/v1/orgs/{}/hooks/{}", org.org.name(), id.0),
        }
    }

    /// `repository` or `organization`, as `ping` payloads name what the hook is on.
    fn ping_context(&self, base_url: &str) -> (&'static str, Value) {
        match self {
            Self::Repository(repo) => ("repository", payloads::repository(&repo.repo, base_url, false)),
            Self::Organization(org) => ("organization", payloads::account(&org.org.account, base_url)),
        }
    }
}

/// A hook's settings, exactly as submitted. For a new hook, [`SecretUpdate::Keep`] means
/// "no secret".
#[derive(Debug, Clone)]
pub struct WebhookInput {
    pub url: String,
    /// Event names (`push`, `repository`, `release`, `fork`); duplicates are ignored.
    pub events: Vec<String>,
    pub active: bool,
    pub secret: SecretUpdate,
}

/// One page of a hook's delivery log.
#[derive(Debug, Clone)]
pub struct DeliveryPage {
    pub deliveries: Vec<Delivery>,
    /// Deliveries across all pages.
    pub total: u64,
}

#[derive(Debug, Clone)]
pub struct WebhookService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    store: WebhookStore,
    audit: AuditService,
    /// `None` if the client could not be built (logged; attempts then record the failure).
    client: Option<reqwest::Client>,
    permits: Arc<Semaphore>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
    /// Signalled when deliveries are queued, so they go out without waiting for a poll.
    wake: Notify,
}

impl WebhookService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let config = &ctx.config.webhooks;
        let timeout = Duration::from_secs(config.timeout_seconds.max(1));
        let client = match delivery::build_client(config.allow_private_addresses, timeout) {
            Ok(client) => Some(client),
            Err(error) => {
                tracing::error!(%error, "cannot build the webhook HTTP client; deliveries will fail");
                None
            }
        };
        Self {
            inner: Arc::new(Inner {
                store: ctx.database.webhooks(),
                audit: AuditService::new(ctx.clone()),
                client,
                permits: Arc::new(Semaphore::new(config.concurrency.max(1))),
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
                wake: Notify::new(),
                ctx,
            }),
        }
    }

    fn base_url(&self) -> &str {
        self.inner.ctx.config.server.base_url()
    }

    fn allow_private(&self) -> bool {
        self.inner.ctx.config.webhooks.allow_private_addresses
    }

    // ------------------------------------------------------------------ management

    /// The target's hooks, oldest first.
    pub async fn list(&self, target: HookTarget<'_>) -> Result<Vec<Webhook>, ServiceError> {
        require_manager(target)?;
        Ok(self.inner.store.list_for(target.owner()).await?)
    }

    /// Hook `id` of the target; [`ServiceError::NotFound`] for hooks of anything else.
    pub async fn get(&self, target: HookTarget<'_>, id: WebhookId) -> Result<Webhook, ServiceError> {
        require_manager(target)?;
        self.inner.store.find(target.owner(), id).await?.ok_or(ServiceError::NotFound)
    }

    /// Creates a hook and queues its `ping`.
    pub async fn create(&self, actor: &User, target: HookTarget<'_>, input: WebhookInput) -> Result<Webhook, ServiceError> {
        require_manager(target)?;
        let owner = target.owner();
        let url = self.validate_url(&input.url)?;
        let events = parse_events(&input.events)?;
        let secret = match input.secret {
            SecretUpdate::Set(secret) => validate_secret(&secret)?,
            SecretUpdate::Keep | SecretUpdate::Clear => None,
        };
        if self.inner.store.count_for(owner).await? >= MAX_HOOKS_PER_OWNER {
            let kind = if matches!(owner, WebhookOwner::Repository(_)) { "repository" } else { "organization" };
            return Err(ServiceError::Invalid(format!("a {kind} can have at most {MAX_HOOKS_PER_OWNER} webhooks; delete one first")));
        }

        let new = NewWebhook { owner, url, secret, events, active: input.active, created_by: Some(actor.id()) };
        let hook = self.inner.store.create(new).await?;
        tracing::info!(target = %target.name(), hook = hook.id.0, host = hook.url.host(), by = %actor.name(), "webhook created");
        self.audit(AuditAction::WebhookCreated, actor, target, &hook).await;
        self.queue_ping(actor, target, &hook).await?;
        Ok(hook)
    }

    /// Replaces hook `id`'s settings.
    pub async fn update(&self, actor: &User, target: HookTarget<'_>, id: WebhookId, input: WebhookInput) -> Result<Webhook, ServiceError> {
        require_manager(target)?;
        let url = self.validate_url(&input.url)?;
        let events = parse_events(&input.events)?;
        let secret = match input.secret {
            SecretUpdate::Set(secret) => match validate_secret(&secret)? {
                Some(secret) => SecretUpdate::Set(secret),
                None => SecretUpdate::Clear,
            },
            other => other,
        };
        let update = WebhookUpdate { url, secret, events, active: input.active };
        let hook = self.inner.store.update(target.owner(), id, update).await?.ok_or(ServiceError::NotFound)?;
        tracing::info!(target = %target.name(), hook = hook.id.0, host = hook.url.host(), by = %actor.name(), "webhook changed");
        self.audit(AuditAction::WebhookChanged, actor, target, &hook).await;
        Ok(hook)
    }

    /// Deletes hook `id` and its delivery log.
    pub async fn delete(&self, actor: &User, target: HookTarget<'_>, id: WebhookId) -> Result<(), ServiceError> {
        let hook = self.get(target, id).await?;
        if !self.inner.store.delete(target.owner(), id).await? {
            return Err(ServiceError::NotFound);
        }
        tracing::info!(target = %target.name(), hook = id.0, by = %actor.name(), "webhook deleted");
        self.audit(AuditAction::WebhookDeleted, actor, target, &hook).await;
        Ok(())
    }

    /// Queues a `ping` to hook `id` (even if it is inactive: someone asked for it).
    pub async fn ping(&self, actor: &User, target: HookTarget<'_>, id: WebhookId) -> Result<Delivery, ServiceError> {
        let hook = self.get(target, id).await?;
        let delivery = self.queue_ping(actor, target, &hook).await?;
        self.inner.store.find_delivery(hook.id, delivery).await?.ok_or(ServiceError::NotFound)
    }

    /// One page (1-based) of hook `id`'s deliveries, newest first.
    pub async fn deliveries(&self, target: HookTarget<'_>, id: WebhookId, page: u32, per_page: u32) -> Result<DeliveryPage, ServiceError> {
        let hook = self.get(target, id).await?;
        let per_page = per_page.max(1);
        let offset = page.max(1).saturating_sub(1).saturating_mul(per_page);
        let deliveries = self.inner.store.list_deliveries(hook.id, per_page, offset).await?;
        let total = u64::try_from(self.inner.store.count_deliveries(hook.id).await?).unwrap_or_default();
        Ok(DeliveryPage { deliveries, total })
    }

    /// Delivery `delivery` of hook `id`.
    pub async fn delivery(&self, target: HookTarget<'_>, id: WebhookId, delivery: DeliveryId) -> Result<Delivery, ServiceError> {
        let hook = self.get(target, id).await?;
        self.inner.store.find_delivery(hook.id, delivery).await?.ok_or(ServiceError::NotFound)
    }

    /// Sends a past delivery again, as a new delivery with the same payload.
    pub async fn redeliver(
        &self,
        actor: &User,
        target: HookTarget<'_>,
        id: WebhookId,
        delivery: DeliveryId,
    ) -> Result<Delivery, ServiceError> {
        let original = self.delivery(target, id, delivery).await?;
        let new = NewDelivery {
            hook_id: original.hook_id,
            guid: delivery::new_guid(),
            event: original.event,
            action: original.action,
            payload: original.payload,
        };
        let ids = self.inner.store.enqueue(vec![new]).await?;
        self.inner.wake.notify_one();
        tracing::info!(target = %target.name(), hook = id.0, delivery = delivery.0, by = %actor.name(), "webhook delivery redelivered");
        let created = ids.first().copied().ok_or(ServiceError::NotFound)?;
        self.inner.store.find_delivery(original.hook_id, created).await?.ok_or(ServiceError::NotFound)
    }

    fn validate_url(&self, input: &str) -> Result<WebhookUrl, ServiceError> {
        let url = WebhookUrl::parse(input).map_err(|error| ServiceError::Invalid(error.to_string()))?;
        if !self.allow_private() && url.targets_internal_host() {
            return Err(ServiceError::Invalid(format!("{} is an internal address; webhooks can only reach public hosts", url.host())));
        }
        Ok(url)
    }

    async fn audit(&self, action: AuditAction, actor: &User, target: HookTarget<'_>, hook: &Webhook) {
        let events: Vec<&str> = hook.events.iter().map(|event| event.as_str()).collect();
        let detail = format!("hook {} to {} ({})", hook.id.0, hook.url.host(), events.join(", "));
        self.inner.audit.record(AuditEntry::new(action).by(actor).on(target.name()).detail(detail)).await;
    }

    async fn queue_ping(&self, actor: &User, target: HookTarget<'_>, hook: &Webhook) -> Result<DeliveryId, ServiceError> {
        let base = self.base_url();
        let hook_json = payloads::hook(hook, &target.hook_api_url(base, hook.id));
        let payload = payloads::ping(&hook_json, hook.id.0, Some(target.ping_context(base)), Some(&actor.account), base);
        let ids = self.queue(vec![hook.id], WebhookEvent::Ping, &payload).await?;
        ids.first().copied().ok_or(ServiceError::NotFound)
    }

    /// Queues `payload` for each of `hooks` and wakes the worker.
    async fn queue(&self, hooks: Vec<WebhookId>, event: WebhookEvent, payload: &Value) -> Result<Vec<DeliveryId>, ServiceError> {
        if hooks.is_empty() {
            return Ok(Vec::new());
        }
        let body = payload.to_string();
        let action = payloads::action_of(event, payload);
        let deliveries = hooks
            .into_iter()
            .map(|hook_id| NewDelivery { hook_id, guid: delivery::new_guid(), event, action: action.clone(), payload: body.clone() })
            .collect();
        let ids = self.inner.store.enqueue(deliveries).await?;
        self.inner.wake.notify_one();
        Ok(ids)
    }

    // ------------------------------------------------------------------ events

    /// Queues deliveries for `event` to every subscribed, active hook. Called by the
    /// background subscriber; public so tests and tools can drive it directly.
    pub async fn handle_event(&self, event: &DomainEvent) -> Result<usize, ServiceError> {
        let base = self.base_url().to_owned();
        let store = &self.inner.store;
        let queued = match event {
            DomainEvent::RepositoryPushed { repo, pusher, updates } => {
                let Some(owned) = self.repository(*repo).await? else { return Ok(0) };
                let hooks = store.subscribed(Some(*repo), &org_of(&owned.owner), WebhookEvent::Push).await?;
                if hooks.is_empty() || updates.is_empty() {
                    return Ok(0);
                }
                let pusher = match pusher {
                    Some(id) => self.account(*id).await?,
                    None => None,
                };
                let reader = self.inner.ctx.repositories.reader(owned.repo.owner_id, owned.repo.id);
                let snapshot = reader.ref_snapshot().await.ok();
                let mut queued = 0;
                for update in updates {
                    let commits = match &update.after {
                        Some(after) if update.branch().is_some() => {
                            let excluding = match (&update.before, &snapshot) {
                                (Some(before), _) => vec![before.clone()],
                                // A new branch: list only what no other ref already has.
                                (None, Some(snapshot)) => other_tips(snapshot.ids(), after),
                                (None, None) => Vec::new(),
                            };
                            reader.new_commits(after.clone(), excluding, MAX_COUNTED_COMMITS).await.unwrap_or_else(|error| {
                                tracing::warn!(%error, repo = %owned.full_name(), r#ref = %update.name, "cannot list pushed commits");
                                Vec::new()
                            })
                        }
                        _ => Vec::new(),
                    };
                    let payload = payloads::PushPayload {
                        repo: &owned,
                        update,
                        commits: &commits,
                        total: commits.len(),
                        pusher: pusher.as_ref(),
                        base_url: &base,
                    }
                    .to_json();
                    queued += self.queue(hooks.clone(), WebhookEvent::Push, &payload).await?.len();
                }
                queued
            }
            DomainEvent::RepositoryCreated { repo, actor } => {
                let Some(owned) = self.repository(*repo).await? else { return Ok(0) };
                let hooks = store.subscribed(Some(*repo), &org_of(&owned.owner), WebhookEvent::Repository).await?;
                let sender = self.account(*actor).await?;
                let repository = payloads::repository(&owned, &base, false);
                let payload =
                    payloads::with_organization(payloads::repository_event("created", &repository, sender.as_ref(), &base), &owned, &base);
                self.queue(hooks, WebhookEvent::Repository, &payload).await?.len()
            }
            DomainEvent::RepositoryDeleted { repo, owner, full_name } => {
                // The repository's own hooks went with it; only its organisation's remain.
                let Some(org) = self.inner.ctx.database.orgs().find(*owner).await? else { return Ok(0) };
                let hooks = store.subscribed(None, &[*owner], WebhookEvent::Repository).await?;
                let repository = payloads::deleted_repository(repo.0, full_name, Some(&org.account), &base);
                let mut payload = payloads::repository_event("deleted", &repository, None, &base);
                payload["organization"] = payloads::account(&org.account, &base);
                self.queue(hooks, WebhookEvent::Repository, &payload).await?.len()
            }
            DomainEvent::RepositoryTransferred { repo, from, to: _ } => {
                let Some(owned) = self.repository(*repo).await? else { return Ok(0) };
                let Some(previous) = self.account(*from).await? else { return Ok(0) };
                let mut orgs = org_of(&owned.owner);
                orgs.extend(org_of(&previous));
                let hooks = store.subscribed(Some(*repo), &orgs, WebhookEvent::Repository).await?;
                let payload = payloads::transferred(&owned, &previous, None, &base);
                self.queue(hooks, WebhookEvent::Repository, &payload).await?.len()
            }
            DomainEvent::RepositoryForked { repo, parent, actor } => {
                let (Some(forkee), Some(origin)) = (self.repository(*repo).await?, self.repository(*parent).await?) else { return Ok(0) };
                let hooks = store.subscribed(Some(*parent), &org_of(&origin.owner), WebhookEvent::Fork).await?;
                let sender = self.account(*actor).await?;
                let payload = payloads::fork(&forkee, &origin, sender.as_ref(), &base);
                self.queue(hooks, WebhookEvent::Fork, &payload).await?.len()
            }
            DomainEvent::ReleasePublished { repo, release, actor } => {
                let Some(owned) = self.repository(*repo).await? else { return Ok(0) };
                let hooks = store.subscribed(Some(*repo), &org_of(&owned.owner), WebhookEvent::Release).await?;
                let sender = self.account(*actor).await?;
                let payload = payloads::release_published(release.0, &owned, sender.as_ref(), &base);
                self.queue(hooks, WebhookEvent::Release, &payload).await?.len()
            }
            _ => 0,
        };
        Ok(queued)
    }

    async fn repository(&self, id: RepoId) -> Result<Option<OwnedRepository>, ServiceError> {
        Ok(self.inner.ctx.database.admin().find_repository(id).await?)
    }

    /// A user or organisation by id.
    async fn account(&self, id: AccountId) -> Result<Option<Account>, ServiceError> {
        if let Some(user) = self.inner.ctx.database.accounts().find_user(id).await? {
            return Ok(Some(user.account));
        }
        Ok(self.inner.ctx.database.orgs().find(id).await?.map(|org| org.account))
    }

    // ------------------------------------------------------------------ background work

    /// Starts the event subscriber, the delivery worker and the daily pruning. Called once
    /// at start-up; later calls do nothing. Does nothing when `[webhooks] enabled = false`.
    pub fn spawn_background(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        if !self.inner.ctx.config.webhooks.enabled {
            tracing::info!("webhook deliveries are disabled");
            return;
        }
        // Subscribe before returning so no event published after start-up is missed.
        let events = self.inner.ctx.events.subscribe();
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.listen(events).await });
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.work().await });
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.prune_periodically().await });
    }

    /// Stops background work: nothing new is claimed, requests in flight are abandoned
    /// and their deliveries returned to the queue, and this waits (bounded) for the tasks
    /// to finish. Call during graceful shutdown, before closing the database.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("webhook deliveries did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn listen(self, mut events: tokio::sync::broadcast::Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(event) => {
                    if let Err(error) = self.handle_event(&event).await {
                        tracing::warn!(%error, ?event, "cannot queue webhook deliveries");
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "webhook subscriber fell behind; some events were not delivered");
                }
                Err(RecvError::Closed) => break,
            }
        }
    }

    async fn work(self) {
        match self.inner.store.release_interrupted().await {
            Ok(0) => {}
            Ok(released) => tracing::info!(released, "re-queued webhook deliveries interrupted by the last shutdown"),
            Err(error) => tracing::warn!(%error, "cannot re-queue interrupted webhook deliveries"),
        }
        loop {
            let Some(permit) = self.acquire().await else { break };
            match self.inner.store.claim_due(Timestamp::now(), 1).await {
                Ok(claimed) => {
                    if let Some(delivery) = claimed.into_iter().next() {
                        let service = self.clone();
                        self.inner.tasks.spawn(async move { service.run(delivery, Some(permit)).await });
                        continue;
                    }
                }
                Err(error) => tracing::warn!(%error, "cannot claim webhook deliveries"),
            }
            drop(permit);
            tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                () = self.inner.wake.notified() => {}
                () = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
    }

    /// Waits for a delivery slot, or `None` once shutdown has begun.
    async fn acquire(&self) -> Option<OwnedSemaphorePermit> {
        tokio::select! {
            () = self.inner.shutdown.cancelled() => None,
            permit = Arc::clone(&self.inner.permits).acquire_owned() => permit.ok(),
        }
    }

    /// Attempts every delivery due at `now` (retries included, until each succeeds or runs
    /// out of attempts) and returns how many attempts were made. The worker does this
    /// continuously; this entry point lets tests and tools drive the queue directly.
    pub async fn deliver_due(&self, now: Timestamp) -> Result<usize, ServiceError> {
        let batch_size = u32::try_from(self.inner.ctx.config.webhooks.concurrency.max(1)).unwrap_or(u32::MAX);
        let mut attempts = 0;
        loop {
            let claimed = self.inner.store.claim_due(now, batch_size).await?;
            if claimed.is_empty() {
                return Ok(attempts);
            }
            attempts += claimed.len();
            let mut running = tokio::task::JoinSet::new();
            for delivery in claimed {
                let service = self.clone();
                running.spawn(async move { service.run(delivery, None).await });
            }
            while running.join_next().await.is_some() {}
        }
    }

    /// One attempt of a claimed delivery, and its bookkeeping.
    async fn run(&self, delivery: ClaimedDelivery, _permit: Option<OwnedSemaphorePermit>) {
        let allow_private = self.allow_private();
        let attempt = tokio::select! {
            () = self.inner.shutdown.cancelled() => {
                if let Err(error) = self.inner.store.release(delivery.id).await {
                    tracing::warn!(%error, delivery = delivery.id.0, "cannot re-queue an interrupted webhook delivery");
                }
                return;
            }
            attempt = delivery::attempt(self.inner.client.as_ref(), &delivery, allow_private) => attempt,
        };

        let failures = delivery.attempts + 1;
        let max_attempts = self.inner.ctx.config.webhooks.max_attempts.max(1);
        let outcome = if attempt.succeeded {
            AttemptOutcome::Succeeded
        } else if attempt.permanent || failures >= max_attempts {
            AttemptOutcome::Failed
        } else {
            let delay = delivery::retry_delay(failures, delivery::random_unit());
            AttemptOutcome::Retry(Timestamp(Timestamp::now().seconds() + i64::try_from(delay.as_secs()).unwrap_or(i64::MAX)))
        };
        let label = match outcome {
            AttemptOutcome::Succeeded => "succeeded",
            AttemptOutcome::Retry(_) => "retrying",
            AttemptOutcome::Failed => "failed",
        };
        metrics::counter!("rustit_webhook_attempts_total", "outcome" => label).increment(1);
        tracing::debug!(
            hook = delivery.hook_id.0,
            delivery = delivery.id.0,
            event = delivery.event.as_str(),
            status = attempt.record.response_status,
            outcome = label,
            "webhook delivery attempted"
        );
        if let Err(error) = self.inner.store.record_attempt(delivery.id, attempt.record, outcome).await {
            tracing::warn!(%error, delivery = delivery.id.0, "cannot record a webhook delivery attempt");
        }
    }

    async fn prune_periodically(self) {
        loop {
            match self.prune().await {
                Ok(0) => {}
                Ok(pruned) => tracing::info!(pruned, "pruned old webhook deliveries"),
                Err(error) => tracing::warn!(%error, "cannot prune webhook deliveries"),
            }
            tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                () = tokio::time::sleep(PRUNE_INTERVAL) => {}
            }
        }
    }

    /// Deletes finished deliveries older than `delivery_retention_days`.
    pub async fn prune(&self) -> Result<u64, ServiceError> {
        let days = i64::from(self.inner.ctx.config.webhooks.delivery_retention_days);
        let cutoff = Timestamp(Timestamp::now().seconds() - days * SECONDS_PER_DAY);
        Ok(self.inner.store.prune(cutoff).await?)
    }
}

fn require_manager(target: HookTarget<'_>) -> Result<(), ServiceError> {
    if target.can_manage() { Ok(()) } else { Err(ServiceError::Forbidden) }
}

/// The organisation among `owner`, if it is one (its hooks fire for its repositories).
fn org_of(owner: &Account) -> Vec<AccountId> {
    if owner.is_organization() { vec![owner.id] } else { Vec::new() }
}

/// Every ref tip except one occurrence of `own` (the new branch itself): if another ref
/// points at the same commit, the branch introduced nothing new.
fn other_tips<'a>(tips: impl Iterator<Item = &'a str>, own: &str) -> Vec<String> {
    let mut skipped = false;
    tips.filter(|tip| {
        if !skipped && *tip == own {
            skipped = true;
            return false;
        }
        true
    })
    .map(str::to_owned)
    .collect()
}

/// Subscribable events, sorted and de-duplicated. `ping` is implicit and dropped.
fn parse_events(names: &[String]) -> Result<Vec<WebhookEvent>, ServiceError> {
    let mut events = Vec::new();
    for name in names {
        let name = name.trim();
        match WebhookEvent::parse(name) {
            Some(WebhookEvent::Ping) => {}
            Some(event) => events.push(event),
            None => return Err(ServiceError::Invalid(format!("unknown event \"{name}\"; choose from push, repository, release and fork"))),
        }
    }
    events.sort_unstable();
    events.dedup();
    if events.is_empty() {
        return Err(ServiceError::Invalid("choose at least one event".to_owned()));
    }
    Ok(events)
}

/// `None` for an empty secret.
fn validate_secret(secret: &str) -> Result<Option<String>, ServiceError> {
    if secret.is_empty() {
        return Ok(None);
    }
    if secret.len() > MAX_SECRET_LEN {
        return Err(ServiceError::Invalid(format!("secret must be at most {MAX_SECRET_LEN} bytes")));
    }
    if secret.chars().any(char::is_control) {
        return Err(ServiceError::Invalid("secret must not contain control characters".to_owned()));
    }
    Ok(Some(secret.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|&value| value.to_owned()).collect()
    }

    #[test]
    fn events_are_validated_sorted_and_deduplicated() {
        assert_eq!(
            parse_events(&names(&["release", "push", "push", " fork "])).unwrap(),
            [WebhookEvent::Push, WebhookEvent::Release, WebhookEvent::Fork]
        );
        assert_eq!(parse_events(&names(&["ping", "repository"])).unwrap(), [WebhookEvent::Repository]);
        assert!(matches!(parse_events(&names(&["issues"])), Err(ServiceError::Invalid(message)) if message.contains("issues")));
        assert!(matches!(parse_events(&names(&["ping"])), Err(ServiceError::Invalid(_))));
        assert!(matches!(parse_events(&[]), Err(ServiceError::Invalid(_))));
    }

    #[test]
    fn a_new_branch_excludes_every_other_tip() {
        let tips = ["a", "b", "c"];
        assert_eq!(other_tips(tips.into_iter(), "b"), ["a", "c"]);
        assert_eq!(other_tips(["a", "b", "b"].into_iter(), "b"), ["a", "b"], "a branch created at another ref's tip adds nothing");
    }

    #[test]
    fn secrets_are_bounded() {
        assert_eq!(validate_secret("").unwrap(), None);
        assert_eq!(validate_secret("s3cret value").unwrap().as_deref(), Some("s3cret value"));
        assert!(validate_secret(&"x".repeat(MAX_SECRET_LEN + 1)).is_err());
        assert!(validate_secret("line\nbreak").is_err());
    }
}
