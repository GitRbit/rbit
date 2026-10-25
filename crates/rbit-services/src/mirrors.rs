//! Pull mirrors: repositories fetched from an upstream URL on a schedule (stretch goal).
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! # Rules
//!
//! * Anyone who may create a repository may create a mirror of a public `http(s)`
//!   upstream ([`rbit_core::mirror::MirrorUrl`]: no credentials) — when
//!   `[mirrors] enabled`. The interval is at least `min_interval_minutes`.
//! * Before every fetch the upstream host is resolved and each address must be public
//!   (not [`rbit_core::network::is_internal_address`]) unless `allow_private_addresses`; the fetch itself refuses
//!   redirects (see [`rbit_git::mirror`]) and pins Git's connection to those approved
//!   addresses, so DNS changes cannot redirect it into the private network.
//! * A fetch replaces the mirror's branches and tags with the upstream's and is reported
//!   through [`PushService`] with `pusher: None`, like any push. Its outcome (time and
//!   error) is stored for the settings page; the next fetch is due an interval later.
//! * Repository administrators change the interval, ask for an immediate fetch, or stop
//!   mirroring (the repository then becomes an ordinary one).
//! * Archived mirrors are frozen: the scheduler skips them and "fetch now" is refused
//!   ([`RepoContext::ensure_writable`]). Unarchiving resumes the schedule; a fetch that
//!   fell due meanwhile runs at once.
//! * Pushes to a mirror are refused by both transports (HTTP and SSH) with
//!   [`PUSH_REFUSED`] — see [`MirrorService::is_mirror`].
//!
//! The scheduler ([`MirrorService::spawn_background`]) wakes every
//! [`SCHEDULER_TICK`] or when a fetch is requested, runs at most [`MAX_CONCURRENT_FETCHES`]
//! fetches at once and never two for the same repository, and stops promptly on
//! [`MirrorService::shutdown`] (a running `git fetch` is killed).

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_core::{
    RepoId, Timestamp, User, Visibility,
    mirror::{Mirror, MirrorUrl},
    repository::OwnedRepository,
};
use rbit_git::{
    RefKind,
    mirror::{FetchError, fetch_mirror, upstream_default_branch},
    remote::ResolvedHost,
};
use rbit_storage::sqlite::{mirrors::MirrorStore, repo_settings::RepoGeneralSettings};
use tokio::sync::{Notify, Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{CreateRepository, PushService, RepoContext, RepositoryService, ServiceContext, ServiceError};

/// How often the scheduler looks for due mirrors.
pub const SCHEDULER_TICK: Duration = Duration::from_secs(30);
/// Fetches running at the same time, instance-wide.
pub const MAX_CONCURRENT_FETCHES: usize = 2;
/// Due mirrors picked up per scheduler pass.
const BATCH: u32 = 16;
/// How long [`MirrorService::shutdown`] waits for running fetches.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Bound for asking the upstream for its default branch.
const HEAD_TIMEOUT: Duration = Duration::from_secs(60);

/// Raw "mirror a repository" input.
#[derive(Debug, Clone, Default)]
pub struct CreateMirror {
    pub name: String,
    pub description: String,
    pub visibility: Visibility,
    pub url: String,
    /// `None` for `[mirrors] default_interval_minutes`.
    pub interval_minutes: Option<u64>,
}

/// What a Git client is told when it pushes to a mirror.
pub const PUSH_REFUSED: &str = "This repository is a mirror; push to its upstream instead";

/// The current stage of an active fetch. Transfer sizes are not available from Git,
/// so callers should show an indeterminate indicator rather than a percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorPhase {
    Connecting,
    Fetching,
    Finalizing,
}

#[derive(Debug, Clone)]
pub struct MirrorService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    repositories: RepositoryService,
    pushes: PushService,
    wake: Notify,
    permits: Arc<Semaphore>,
    running: Mutex<HashSet<RepoId>>,
    phases: Mutex<HashMap<RepoId, MirrorPhase>>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl MirrorService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let repositories =
            RepositoryService::new(Arc::clone(&ctx.config), ctx.database.repositories(), ctx.repositories.clone(), ctx.events.clone());
        Self {
            inner: Arc::new(Inner {
                pushes: PushService::new(ctx.clone()),
                repositories,
                wake: Notify::new(),
                permits: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES)),
                running: Mutex::new(HashSet::new()),
                phases: Mutex::new(HashMap::new()),
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
                ctx,
            }),
        }
    }

    fn store(&self) -> MirrorStore {
        self.inner.ctx.database.mirrors()
    }

    /// Whether mirrors can be created on this instance.
    pub fn is_enabled(&self) -> bool {
        self.inner.ctx.config.mirrors.enabled
    }

    pub fn default_interval_minutes(&self) -> u64 {
        self.inner.ctx.config.mirrors.default_interval_minutes
    }

    pub fn min_interval_minutes(&self) -> u64 {
        self.inner.ctx.config.mirrors.min_interval_minutes
    }

    fn checked_interval(&self, minutes: Option<u64>) -> Result<u64, ServiceError> {
        let minutes = minutes.unwrap_or_else(|| self.default_interval_minutes());
        let min = self.min_interval_minutes();
        if minutes < min || minutes > 60 * 24 * 365 {
            return Err(ServiceError::Invalid(format!("the interval must be between {min} minutes and a year")));
        }
        Ok(minutes)
    }

    /// Checks that every address of the upstream's host is public (unless private
    /// addresses are allowed).
    async fn check_upstream(&self, url: &MirrorUrl) -> Result<ResolvedHost, ServiceError> {
        crate::push_mirrors::check_public_host(url.host(), url.port(), self.inner.ctx.config.mirrors.allow_private_addresses)
            .await
            .map_err(ServiceError::Invalid)
    }

    // ------------------------------------------------------------------ management

    /// Creates `owner/name` as a mirror of `input.url`; the first fetch starts right away.
    pub async fn create(&self, actor: &User, owner: &str, input: CreateMirror) -> Result<OwnedRepository, ServiceError> {
        if !self.is_enabled() {
            return Err(ServiceError::Invalid("mirrors are disabled on this instance".to_owned()));
        }
        let url: MirrorUrl = input.url.parse().map_err(|e: rbit_core::mirror::MirrorUrlError| ServiceError::Invalid(e.to_string()))?;
        let interval = self.checked_interval(input.interval_minutes)?;
        self.check_upstream(&url).await?;
        let repository = CreateRepository { name: input.name, description: input.description, visibility: input.visibility };
        let created = self.inner.repositories.create_in(actor, owner, repository).await?;
        if let Err(error) = self.store().create(created.repo.id, url.as_str(), interval).await {
            tracing::error!(%error, "cannot record a new mirror; the repository stays an ordinary one");
            return Err(error.into());
        }
        tracing::info!(repo = %created.full_name(), url = %url, "mirror created");
        self.inner.wake.notify_one();
        Ok(created)
    }

    /// The mirror settings of `repo`, if it is a mirror.
    pub async fn find(&self, repo: &RepoContext) -> Result<Option<Mirror>, ServiceError> {
        Ok(self.store().find(repo.repo.repo.id).await?)
    }

    /// Whether `repo` is a mirror. The Git transports must refuse pushes to mirrors.
    pub async fn is_mirror(&self, repo: RepoId) -> Result<bool, ServiceError> {
        Ok(self.store().is_mirror(repo).await?)
    }

    pub fn active_phase(&self, repo: RepoId) -> Option<MirrorPhase> {
        lock(&self.inner.phases).get(&repo).copied()
    }

    fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
        if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::NotFound) }
    }

    pub async fn set_interval(&self, repo: &RepoContext, minutes: u64) -> Result<(), ServiceError> {
        Self::require_admin(repo)?;
        let minutes = self.checked_interval(Some(minutes))?;
        self.store().find(repo.repo.repo.id).await?.ok_or(ServiceError::NotFound)?;
        self.store().set_interval(repo.repo.repo.id, minutes).await?;
        self.inner.wake.notify_one();
        Ok(())
    }

    /// Schedules a fetch now (it runs in the background).
    pub async fn sync_now(&self, repo: &RepoContext) -> Result<(), ServiceError> {
        Self::require_admin(repo)?;
        repo.ensure_writable()?;
        self.store().find(repo.repo.repo.id).await?.ok_or(ServiceError::NotFound)?;
        self.store().schedule_now(repo.repo.repo.id).await?;
        self.inner.wake.notify_one();
        Ok(())
    }

    /// Stops mirroring; the repository keeps its content and accepts pushes again.
    pub async fn remove(&self, repo: &RepoContext) -> Result<(), ServiceError> {
        Self::require_admin(repo)?;
        self.store().delete(repo.repo.repo.id).await?;
        tracing::info!(repo = %repo.repo.full_name(), "mirroring stopped");
        Ok(())
    }

    // ------------------------------------------------------------------ fetching

    /// Fetches mirror `repo` now and records the outcome. Errors that concern the
    /// upstream (unreachable, refused address, Git failure) are recorded, not returned.
    pub async fn sync(&self, repo: RepoId) -> Result<(), ServiceError> {
        let Some(mirror) = self.store().find(repo).await? else { return Ok(()) };
        let Some(context) = RepoContext::load_for_system(&self.inner.ctx, repo).await? else { return Ok(()) };
        if context.repo.repo.is_archived() {
            // Archived mirrors are frozen: they fetch again once unarchived.
            return Ok(());
        }
        // Clear transient progress on success, errors and cancellation alike.
        let _progress = ActiveProgress { phases: &self.inner.phases, repo };
        lock(&self.inner.phases).insert(repo, MirrorPhase::Connecting);
        let outcome = self.fetch(&context, &mirror).await;
        let error = outcome.as_ref().err().map(ToString::to_string);
        match &error {
            None => tracing::info!(repo = %context.repo.full_name(), "mirror fetched"),
            Some(error) => tracing::warn!(repo = %context.repo.full_name(), %error, "mirror fetch failed"),
        }
        metrics::counter!("rbit_mirror_fetches_total", "outcome" => if error.is_none() { "ok" } else { "error" }).increment(1);
        self.store().record_sync(repo, Timestamp::now(), error.as_deref()).await?;
        Ok(())
    }

    async fn fetch(&self, context: &RepoContext, mirror: &Mirror) -> Result<(), String> {
        let resolution = self.check_upstream(&mirror.url).await.map_err(|e| e.to_string())?;
        let config = &self.inner.ctx.config;
        let git = &config.repository.git_binary;
        let timeout = Duration::from_secs(config.mirrors.fetch_timeout_seconds);
        let pending = self.inner.pushes.begin(context, None).await;
        let fetched = {
            // `git fetch` writes into the repository, so a transfer waits for it.
            let _lease = self.inner.ctx.repositories.lease(context.repo.repo.id).await;
            lock(&self.inner.phases).insert(context.repo.repo.id, MirrorPhase::Fetching);
            fetch_mirror(git, &context.disk_path, mirror.url.as_str(), Some(&resolution), timeout).await
        };
        lock(&self.inner.phases).insert(context.repo.repo.id, MirrorPhase::Finalizing);
        match fetched {
            Ok(()) => self.inner.pushes.complete(pending).await,
            Err(error) => return Err(describe(&error)),
        }
        self.adopt_default_branch(context, mirror, &resolution).await;
        Ok(())
    }

    /// After a fetch, when the repository's default branch does not exist (a new mirror
    /// of an upstream whose default is, say, `master`), adopt the upstream's.
    async fn adopt_default_branch(&self, context: &RepoContext, mirror: &Mirror, resolution: &ResolvedHost) {
        let current = &context.repo.repo;
        let Ok(refs) = context.reader.refs().await else { return };
        let branches: Vec<&str> = refs.iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name.as_str()).collect();
        if branches.is_empty() || branches.contains(&current.default_branch.as_str()) {
            return;
        }
        let git = &self.inner.ctx.config.repository.git_binary;
        let upstream = upstream_default_branch(git, mirror.url.as_str(), Some(resolution), HEAD_TIMEOUT).await.ok().flatten();
        let Some(branch) = upstream.filter(|b| branches.contains(&b.as_str())).or_else(|| branches.first().map(|b| (*b).to_owned())) else {
            return;
        };
        let settings = RepoGeneralSettings {
            description: current.description.clone(),
            visibility: current.visibility,
            default_branch: branch.clone(),
        };
        let updated = async {
            self.inner.ctx.repositories.set_head(current.owner_id, current.id, &branch).await?;
            self.inner.ctx.database.repo_settings().update_general(current.id, &settings).await?;
            Ok::<_, ServiceError>(())
        };
        if let Err(error) = updated.await {
            tracing::warn!(%error, repo = %context.repo.full_name(), "cannot adopt the upstream's default branch");
        }
    }

    // ------------------------------------------------------------------ background

    /// Starts the scheduler (when mirrors are enabled). Called once at start-up; later
    /// calls do nothing.
    pub fn spawn_background(&self) {
        if !self.is_enabled() || self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.schedule().await });
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("mirror fetches did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn schedule(self) {
        loop {
            if let Err(error) = self.start_due().await {
                tracing::warn!(%error, "cannot look for due mirrors");
            }
            tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                () = self.inner.wake.notified() => {}
                () = tokio::time::sleep(SCHEDULER_TICK) => {}
            }
        }
    }

    /// Starts a fetch for every due mirror that is not already being fetched.
    async fn start_due(&self) -> Result<(), ServiceError> {
        for mirror in self.store().due(Timestamp::now(), BATCH).await? {
            let repo = mirror.repo_id;
            if !lock(&self.inner.running).insert(repo) {
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
                        result = service.sync(repo) => {
                            if let Err(error) = result {
                                tracing::warn!(%error, repo = repo.0, "mirror sync failed");
                            }
                        }
                    }
                }
                lock(&inner.running).remove(&repo);
            });
        }
        Ok(())
    }
}

struct ActiveProgress<'a> {
    phases: &'a Mutex<HashMap<RepoId, MirrorPhase>>,
    repo: RepoId,
}

impl Drop for ActiveProgress<'_> {
    fn drop(&mut self) {
        lock(self.phases).remove(&self.repo);
    }
}

/// A fetch failure as shown on the settings page.
fn describe(error: &FetchError) -> String {
    match error {
        FetchError::Git(error) => {
            tracing::error!(%error, "mirror fetch could not run");
            "the fetch could not run; see the server log".to_owned()
        }
        other => other.to_string(),
    }
}

/// Locks a mutex, recovering the data if a previous holder panicked (the set stays valid).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
