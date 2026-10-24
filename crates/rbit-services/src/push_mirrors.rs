//! Push mirrors: remotes a repository force-pushes its branches and tags to.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! # Rules
//!
//! * Repository administrators manage a repository's push mirrors (at most
//!   [`MAX_PUSH_MIRRORS`]); to everyone else they do not exist (404). Only with
//!   `[push_mirrors] enabled`.
//! * A remote is an `http(s)` URL without credentials; the user name and secret are
//!   separate fields. The secret is stored as given (git must send it), like webhook
//!   secrets, and is **write-only**: never shown, returned or logged; leaving it blank when
//!   editing keeps it. `file://` remotes exist only behind `[push_mirrors]
//!   allow_local_urls`, a test switch.
//! * Before every push the remote's host is resolved and each address must be public
//!   (not [`is_internal_address`]) unless `allow_private_addresses`, as for pull mirrors;
//!   git then refuses redirects (see [`rbit_git::remote`]). The same check runs when a
//!   remote is saved, for an early answer.
//! * A push is `git push --force --prune` of `refs/heads/*` and `refs/tags/*`: the remote
//!   ends up with exactly this repository's branches and tags (pull request heads stay
//!   here). Credentials reach git as an `Authorization: Basic` header through its
//!   environment, never in arguments or URLs; an empty user name is sent as `git`.
//! * Mirrors that sync on push are pushed [`PushMirrorConfig::debounce_seconds`] after a
//!   push to the repository ([`DomainEvent::RepositoryPushed`]); a burst of pushes is sent
//!   once. Every enabled mirror is also pushed every `interval_minutes`, and an
//!   administrator can ask for a push now. The time, outcome and error of the last attempt
//!   are kept for the settings page.
//! * Archived repositories are not pushed: nothing changes in them, so their mirrors
//!   pause (the scheduler skips them and "push now" is refused) until they are
//!   unarchived.
//!
//! The scheduler ([`PushMirrorService::spawn_background`]) wakes every
//! [`SCHEDULER_TICK`], after a debounce, or when a push is requested; it runs at most
//! [`MAX_CONCURRENT_PUSHES`] pushes at once and never two for the same mirror, and stops
//! promptly on [`PushMirrorService::shutdown`] (a running `git push` is killed).
//!
//! [`PushMirrorConfig::debounce_seconds`]: rbit_core::config::PushMirrorConfig

use std::{
    collections::HashSet,
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use rbit_core::{
    RepoId, Timestamp, User,
    network::is_internal_address,
    push_mirror::{MAX_PUSH_MIRRORS, PushMirror, PushMirrorId, PushMirrorUrl, validate_secret, validate_username},
};
use rbit_git::remote::{Remote, RemoteError, ResolvedHost, push_mirror};
use rbit_storage::sqlite::push_mirrors::{NewPushMirror, PushMirrorChanges, PushMirrorStore, PushTarget};
use tokio::sync::{Notify, Semaphore, broadcast::error::RecvError};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{DomainEvent, RepoContext, ServiceContext, ServiceError};

/// How often the scheduler looks for due mirrors.
pub const SCHEDULER_TICK: Duration = Duration::from_secs(30);
/// Pushes running at the same time, instance-wide.
pub const MAX_CONCURRENT_PUSHES: usize = 2;
/// Due mirrors picked up per scheduler pass.
const BATCH: u32 = 16;
/// How long [`PushMirrorService::shutdown`] waits for running pushes.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// The user name sent when none is configured.
const DEFAULT_USERNAME: &str = "git";

/// Raw push mirror form input.
#[derive(Debug, Clone, Default)]
pub struct PushMirrorInput {
    pub url: String,
    pub username: String,
    /// Empty: none (when creating) or unchanged (when editing).
    pub secret: String,
    pub enabled: bool,
    pub sync_on_push: bool,
}

#[derive(Debug, Clone)]
pub struct PushMirrorService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    wake: Notify,
    permits: Arc<Semaphore>,
    running: Mutex<HashSet<PushMirrorId>>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl PushMirrorService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self {
            inner: Arc::new(Inner {
                ctx,
                wake: Notify::new(),
                permits: Arc::new(Semaphore::new(MAX_CONCURRENT_PUSHES)),
                running: Mutex::new(HashSet::new()),
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
            }),
        }
    }

    fn store(&self) -> PushMirrorStore {
        self.inner.ctx.database.push_mirrors()
    }

    fn config(&self) -> &rbit_core::config::PushMirrorConfig {
        &self.inner.ctx.config.push_mirrors
    }

    /// Whether push mirrors can be configured and are pushed on this instance.
    pub fn is_enabled(&self) -> bool {
        self.config().enabled
    }

    /// How often every mirror is pushed.
    pub fn interval_minutes(&self) -> u64 {
        self.config().interval_minutes.max(1)
    }

    fn require_admin(&self, repo: &RepoContext) -> Result<(), ServiceError> {
        if self.is_enabled() && repo.access.can_admin() { Ok(()) } else { Err(ServiceError::NotFound) }
    }

    /// Validates `input` (the URL, the credentials and the remote's address).
    async fn validated(&self, input: &PushMirrorInput) -> Result<PushMirrorUrl, ServiceError> {
        let invalid = |e: rbit_core::push_mirror::PushMirrorError| ServiceError::Invalid(e.to_string());
        let url = PushMirrorUrl::parse(&input.url, self.config().allow_local_urls).map_err(invalid)?;
        validate_username(input.username.trim()).map_err(invalid)?;
        validate_secret(&input.secret).map_err(invalid)?;
        self.check_remote(&url).await.map_err(ServiceError::Invalid)?;
        Ok(url)
    }

    async fn check_remote(&self, url: &PushMirrorUrl) -> Result<Option<ResolvedHost>, String> {
        match url.remote() {
            Some(remote) => check_public_host(remote.host(), remote.port(), self.config().allow_private_addresses).await.map(Some),
            None => Ok(None),
        }
    }

    // ------------------------------------------------------------------ management

    /// The mirrors of `repo` (administrators only).
    pub async fn list(&self, repo: &RepoContext) -> Result<Vec<PushMirror>, ServiceError> {
        self.require_admin(repo)?;
        Ok(self.store().list(repo.repo.repo.id).await?)
    }

    /// Adds a mirror; its first push starts right away.
    pub async fn create(&self, repo: &RepoContext, actor: &User, input: &PushMirrorInput) -> Result<PushMirror, ServiceError> {
        self.require_admin(repo)?;
        let repo_id = repo.repo.repo.id;
        if self.store().count(repo_id).await? >= u64::try_from(MAX_PUSH_MIRRORS).unwrap_or(u64::MAX) {
            return Err(ServiceError::Invalid(format!("a repository can push to at most {MAX_PUSH_MIRRORS} mirrors")));
        }
        let url = self.validated(input).await?;
        let mirror = self
            .store()
            .create(&NewPushMirror {
                repo: repo_id,
                url: url.as_str(),
                username: input.username.trim(),
                secret: (!input.secret.is_empty()).then_some(input.secret.as_str()),
                sync_on_push: input.sync_on_push,
                created_by: actor.id(),
            })
            .await?;
        tracing::info!(repo = %repo.repo.full_name(), mirror = mirror.id.0, url = %url, "push mirror added");
        self.inner.wake.notify_one();
        Ok(mirror)
    }

    /// Changes a mirror's settings; a blank secret keeps the stored one.
    pub async fn update(&self, repo: &RepoContext, id: PushMirrorId, input: &PushMirrorInput) -> Result<PushMirror, ServiceError> {
        self.require_admin(repo)?;
        let repo_id = repo.repo.repo.id;
        self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        let url = self.validated(input).await?;
        let changes = PushMirrorChanges {
            url: url.as_str(),
            username: input.username.trim(),
            secret: (!input.secret.is_empty()).then_some(input.secret.as_str()),
            enabled: input.enabled,
            sync_on_push: input.sync_on_push,
        };
        if !self.store().update(repo_id, id, &changes).await? {
            return Err(ServiceError::NotFound);
        }
        self.inner.wake.notify_one();
        self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)
    }

    pub async fn delete(&self, repo: &RepoContext, id: PushMirrorId) -> Result<(), ServiceError> {
        self.require_admin(repo)?;
        if !self.store().delete(repo.repo.repo.id, id).await? {
            return Err(ServiceError::NotFound);
        }
        tracing::info!(repo = %repo.repo.full_name(), mirror = id.0, "push mirror removed");
        Ok(())
    }

    /// Schedules a push now (it runs in the background).
    pub async fn sync_now(&self, repo: &RepoContext, id: PushMirrorId) -> Result<(), ServiceError> {
        self.require_admin(repo)?;
        repo.ensure_writable()?;
        if !self.store().schedule_now(repo.repo.repo.id, id).await? {
            return Err(ServiceError::NotFound);
        }
        self.inner.wake.notify_one();
        Ok(())
    }

    // ------------------------------------------------------------------ pushing

    /// Pushes mirror `id` now and records the outcome. Errors that concern the remote
    /// (unreachable, refused address, rejected credentials) are recorded, not returned.
    pub async fn push(&self, id: PushMirrorId) -> Result<(), ServiceError> {
        let Some(target) = self.store().target(id).await? else { return Ok(()) };
        let next = Timestamp(Timestamp::now().seconds().saturating_add(i64::try_from(self.interval_minutes() * 60).unwrap_or(i64::MAX)));
        self.store().claim(id, next).await?;
        let Some(context) = RepoContext::load_for_system(&self.inner.ctx, target.mirror.repo_id).await? else { return Ok(()) };
        if context.repo.repo.is_archived() {
            // Archived since it was picked: it resumes once the repository is unarchived.
            return Ok(());
        }
        let outcome = self.attempt(&context, &target).await;
        let error = outcome.err();
        match &error {
            None => tracing::info!(repo = %context.repo.full_name(), mirror = id.0, "push mirror updated"),
            Some(error) => tracing::warn!(repo = %context.repo.full_name(), mirror = id.0, %error, "push mirror failed"),
        }
        metrics::counter!("rbit_push_mirror_pushes_total", "outcome" => if error.is_none() { "ok" } else { "error" }).increment(1);
        self.store().record_attempt(id, Timestamp::now(), error.as_deref()).await?;
        Ok(())
    }

    async fn attempt(&self, context: &RepoContext, target: &PushTarget) -> Result<(), String> {
        let config = self.config();
        let url = PushMirrorUrl::parse(&target.mirror.url, config.allow_local_urls).map_err(|e| e.to_string())?;
        let resolution = self.check_remote(&url).await?;
        let authorization = target.secret.as_deref().map(|secret| {
            let username = Some(target.mirror.username.as_str()).filter(|name| !name.is_empty()).unwrap_or(DEFAULT_USERNAME);
            basic_authorization(username, secret)
        });
        // An empty repository has nothing to mirror yet (git would refuse the push).
        let refs = context.reader.refs().await.map_err(|error| {
            tracing::error!(%error, repo = %context.repo.full_name(), "cannot read refs for a push mirror");
            "the repository could not be read; see the server log".to_owned()
        })?;
        if refs.is_empty() {
            return Ok(());
        }
        let remote = Remote {
            url: url.as_str(),
            authorization: authorization.as_deref(),
            allow_local: config.allow_local_urls,
            resolution: resolution.as_ref(),
        };
        let git = &self.inner.ctx.config.repository.git_binary;
        let timeout = Duration::from_secs(config.push_timeout_seconds.max(1));
        // A transfer moves the repository's directory; it waits for the push.
        let _lease = self.inner.ctx.repositories.lease(context.repo.repo.id).await;
        push_mirror(git, &context.disk_path, remote, timeout).await.map_err(|error| describe(&error))
    }

    // ------------------------------------------------------------------ background

    /// Starts the scheduler and the push subscriber (when push mirrors are enabled).
    /// Called once at start-up; later calls do nothing.
    pub fn spawn_background(&self) {
        if !self.is_enabled() || self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.schedule().await });
        let service = self.clone();
        let events = self.inner.ctx.events.subscribe();
        self.inner.tasks.spawn(async move { service.follow_pushes(events).await });
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("push mirrors did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    /// Schedules the push mirrors of every pushed repository after the debounce delay.
    async fn follow_pushes(self, mut events: tokio::sync::broadcast::Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(DomainEvent::RepositoryPushed { repo, updates, .. }) if !updates.is_empty() => self.pushed(repo).await,
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "push mirrors missed push events; the interval catches up"),
                Err(RecvError::Closed) => break,
            }
        }
    }

    async fn pushed(&self, repo: RepoId) {
        let delay = Duration::from_secs(self.config().debounce_seconds);
        let at = Timestamp(Timestamp::now().seconds().saturating_add(i64::try_from(delay.as_secs()).unwrap_or(i64::MAX)));
        match self.store().schedule_after_push(repo, at).await {
            Ok(0) => {}
            Ok(_) => {
                let service = self.clone();
                self.inner.tasks.spawn(async move {
                    tokio::select! {
                        () = service.inner.shutdown.cancelled() => {}
                        () = tokio::time::sleep(delay) => service.inner.wake.notify_one(),
                    }
                });
            }
            Err(error) => tracing::warn!(%error, repo = repo.0, "cannot schedule push mirrors after a push"),
        }
    }

    async fn schedule(self) {
        loop {
            if let Err(error) = self.start_due().await {
                tracing::warn!(%error, "cannot look for due push mirrors");
            }
            tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                () = self.inner.wake.notified() => {}
                () = tokio::time::sleep(SCHEDULER_TICK) => {}
            }
        }
    }

    /// Starts a push for every due mirror that is not already being pushed.
    async fn start_due(&self) -> Result<(), ServiceError> {
        for mirror in self.store().due(Timestamp::now(), BATCH).await? {
            let id = mirror.id;
            if !lock(&self.inner.running).insert(id) {
                continue;
            }
            let service = self.clone();
            self.inner.tasks.spawn(async move {
                let inner = &service.inner;
                let permit = tokio::select! {
                    () = inner.shutdown.cancelled() => None,
                    permit = Arc::clone(&inner.permits).acquire_owned() => permit.ok(),
                };
                if permit.is_some() {
                    tokio::select! {
                        () = inner.shutdown.cancelled() => {}
                        result = service.push(id) => {
                            if let Err(error) = result {
                                tracing::warn!(%error, mirror = id.0, "push mirror attempt failed");
                            }
                        }
                    }
                }
                lock(&inner.running).remove(&id);
            });
        }
        Ok(())
    }
}

/// `Basic <base64(username:secret)>`, the header value git sends.
pub(crate) fn basic_authorization(username: &str, secret: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{username}:{secret}")))
}

/// Checks that every address `host` resolves to is public (unless `allow_private`), as
/// pull mirrors do before fetching. The error is shown to the user.
pub(crate) async fn check_public_host(host: &str, port: u16, allow_private: bool) -> Result<ResolvedHost, String> {
    // URL clients recognize shortened, hexadecimal and octal IPv4 spellings before
    // DNS. Classify their canonical address rather than relying on platform-specific
    // getaddrinfo behavior, and use the canonical IDNA hostname for DNS pinning.
    let canonical = if host.parse::<IpAddr>().is_ok() {
        host.to_owned()
    } else {
        let parsed = reqwest::Url::parse(&format!("http://{host}:{port}/")).map_err(|_| format!("cannot resolve {host}"))?;
        parsed.host_str().ok_or_else(|| format!("cannot resolve {host}"))?.to_owned()
    };
    let host = canonical.as_str();
    let mut addresses: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(address) => vec![address],
        Err(_) => {
            tokio::net::lookup_host((host, port)).await.map_err(|_| format!("cannot resolve {host}"))?.map(|socket| socket.ip()).collect()
        }
    };
    if addresses.is_empty() {
        return Err(format!("cannot resolve {host}"));
    }
    if !allow_private && addresses.iter().any(|address| is_internal_address(*address)) {
        return Err(format!("{host} is not a public address; only servers on the internet can be reached"));
    }
    addresses.sort_unstable();
    addresses.dedup();
    ResolvedHost::new(host, port, addresses).map_err(|_| format!("cannot resolve {host}"))
}

/// A push failure as shown on the settings page.
fn describe(error: &RemoteError) -> String {
    match error {
        RemoteError::Git(error) => {
            tracing::error!(%error, "a push mirror could not run");
            "the push could not run; see the server log".to_owned()
        }
        other => other.to_string(),
    }
}

/// Locks a mutex, recovering the data if a previous holder panicked (the set stays valid).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_become_a_basic_header() {
        assert_eq!(basic_authorization("bot", "s3cret"), "Basic Ym90OnMzY3JldA==");
    }

    #[tokio::test]
    async fn internal_hosts_are_refused_unless_allowed() {
        assert!(check_public_host("127.0.0.1", 443, false).await.is_err());
        assert!(check_public_host("::1", 443, false).await.is_err());
        assert!(check_public_host("10.1.2.3", 443, false).await.is_err());
        assert!(check_public_host("127.0.0.1", 443, true).await.is_ok());
        assert!(check_public_host("93.184.216.34", 443, false).await.is_ok());
        for host in ["2130706433", "0x7f000001", "127.1", "0177.0.0.1"] {
            assert!(check_public_host(host, 443, false).await.is_err(), "{host}");
            assert!(check_public_host(host, 443, true).await.is_ok(), "{host}");
        }
    }
}
