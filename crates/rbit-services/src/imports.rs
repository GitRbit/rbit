//! Importing repositories from GitHub (or GitHub Enterprise) and Gitea/Forgejo.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! # Rules
//!
//! * Anyone who may create a repository may import one into an owner they can create
//!   repositories for (themselves or one of their organisations) — when `[imports]
//!   enabled`. The source is `owner/name` on github.com, a GitHub Enterprise server or a
//!   Gitea/Forgejo server ([`ImportSource`]); a token is optional for public sources.
//! * Before anything else the source's API host must resolve to public addresses only
//!   (unless `allow_private_addresses`), and the repository's metadata must be readable:
//!   a wrong name or token is reported on the form, before a repository is created. Every
//!   later request — Git, API, release files, LFS objects — is checked the same way (see
//!   [`client`] and [`rbit_git::remote`]).
//! * The import then runs in the background ([`job`]) and shows its phase, counts,
//!   warnings and error on `/{owner}/{repo}/import` to the importer and the repository's
//!   administrators ([`ImportService::status`]); to everyone else that page does not
//!   exist. A failed import leaves the repository with everything imported so far.
//! * **The token is never stored.** It lives in the job's memory (and reaches git only
//!   through its environment) until the job ends. An import that a restart interrupted
//!   therefore cannot resume: start-up marks it failed, and it can be repeated into a new
//!   repository.
//! * Issue and pull request numbers are kept exactly; numbers for threads created here
//!   during the import are reserved past the source's highest. Authors become local
//!   accounts only when `[imports] author_mapping` says so (by verified email address,
//!   optionally by user name); otherwise the importing user writes everything and each
//!   text starts with "Originally by @login on GitHub", linked to the original profile.
//! * Imported history sends no notifications, emails or webhooks (see [`job`]).
//!
//! At most `[imports] max_concurrent` imports run at once; more wait as `queued`.
//! [`ImportService::shutdown`] stops running imports promptly (a running `git fetch` is
//! killed) and records why.

pub mod api;
pub mod client;
mod job;

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_core::{
    Config, RepoId, Timestamp, User, Visibility,
    import::{ImportOptions, ImportSource, RepoImport, SourceKind, truncate_chars},
    repository::OwnedRepository,
};
use rbit_storage::sqlite::imports::{ImportStore, NewImport};
use tokio::sync::Semaphore;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use self::{
    client::SourceClient,
    job::{Job, JobServices},
};
use crate::{
    CreateRepository, LfsService, ReleaseService, RepositoryService, ServiceContext, ServiceError, push_mirrors::check_public_host,
};

/// How long [`ImportService::shutdown`] waits for running imports to record their end.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Longest token accepted.
const MAX_TOKEN_BYTES: usize = 1024;
/// Longest repository description (as [`RepositoryService`] accepts).
const MAX_DESCRIPTION_CHARS: usize = 350;
/// Recorded for imports a restart interrupted.
pub const INTERRUPTED: &str = "the import was interrupted when the server restarted; the token is not kept, so start a new import";
/// Recorded for imports the server stopped.
pub const STOPPED: &str = "the import was stopped because the server shut down";

/// Raw "import a repository" input.
#[derive(Clone, Default)]
pub struct ImportInput {
    pub kind: SourceKind,
    /// The server's address (not for github.com).
    pub host: String,
    /// `owner/name` or the repository's URL on the source.
    pub repository: String,
    /// A personal access token, or empty for public sources.
    pub token: String,
    /// The local owner (the importer or one of their organisations).
    pub owner: String,
    /// The local name; empty for the source's.
    pub name: String,
    pub visibility: Visibility,
    pub options: ImportOptions,
}

impl std::fmt::Debug for ImportInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportInput")
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("repository", &self.repository)
            .field("token", &if self.token.is_empty() { "" } else { "…" })
            .field("owner", &self.owner)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct ImportService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    repositories: RepositoryService,
    lfs: LfsService,
    releases: ReleaseService,
    permits: Arc<Semaphore>,
    /// Repositories whose import this process runs (queued or running).
    running: Mutex<HashSet<RepoId>>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl ImportService {
    pub(crate) fn new(ctx: ServiceContext, repositories: RepositoryService, lfs: LfsService, releases: ReleaseService) -> Self {
        let permits = Arc::new(Semaphore::new(ctx.config.imports.max_concurrent.max(1)));
        Self {
            inner: Arc::new(Inner {
                ctx,
                repositories,
                lfs,
                releases,
                permits,
                running: Mutex::new(HashSet::new()),
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
            }),
        }
    }

    fn store(&self) -> ImportStore {
        self.inner.ctx.database.imports()
    }

    /// Whether importing is offered on this instance.
    pub fn is_enabled(&self) -> bool {
        self.inner.ctx.config.imports.enabled
    }

    /// Validates `input`, reads the source repository's metadata, creates the local
    /// repository and starts the import in the background. Problems with the source
    /// (unknown repository, refused token, internal address) are [`ServiceError::Invalid`]
    /// and nothing is created.
    pub async fn start(&self, actor: &User, input: ImportInput) -> Result<OwnedRepository, ServiceError> {
        if !self.is_enabled() {
            return Err(ServiceError::Invalid("importing is disabled on this instance".to_owned()));
        }
        let config = &self.inner.ctx.config;
        let source = ImportSource::parse(input.kind, &input.host, &input.repository).map_err(|e| ServiceError::Invalid(e.to_string()))?;
        let token = input.token.trim();
        if token.len() > MAX_TOKEN_BYTES || token.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(ServiceError::Invalid("the token is not valid".to_owned()));
        }
        check_source_host(config, &source.api_host, source.api_port).await.map_err(ServiceError::Invalid)?;
        let client = SourceClient::new(
            source.clone(),
            (!token.is_empty()).then(|| token.to_owned()),
            config.imports.allow_private_addresses,
            Duration::from_secs(config.imports.api_timeout_seconds.max(1)),
            Duration::from_secs(config.imports.rate_limit_max_wait_seconds),
            self.inner.shutdown.child_token(),
        )
        .map_err(|e| ServiceError::Invalid(e.to_string()))?;
        let metadata = client.repository().await.map_err(|e| ServiceError::Invalid(format!("{}: {e}", source.full_name())))?;

        let name = match input.name.trim() {
            "" => source.name.clone(),
            name => name.to_owned(),
        };
        let description =
            metadata.description.as_deref().map(|d| truncate_chars(d.trim(), MAX_DESCRIPTION_CHARS).to_owned()).unwrap_or_default();
        let repository = CreateRepository { name, description, visibility: input.visibility };
        let created = self.inner.repositories.create_in(actor, &input.owner, repository).await?;
        let repo_id = created.repo.id;

        // Marked as this process's before the record exists, so start-up's sweep of
        // interrupted imports cannot catch it.
        lock(&self.inner.running).insert(repo_id);
        let source_url = source.web_url();
        let record = NewImport { repo: repo_id, importer: actor.id(), kind: source.kind, source_url: &source_url, options: input.options };
        if let Err(error) = self.store().create(&record).await {
            lock(&self.inner.running).remove(&repo_id);
            tracing::error!(%error, "cannot record an import; the repository stays empty");
            return Err(error.into());
        }
        tracing::info!(repo = %created.full_name(), source = %source_url, "import queued");
        self.spawn(created.clone(), actor.clone(), client, metadata, input.options);
        Ok(created)
    }

    /// The import of `repo` for `viewer`: its importer or an administrator of the
    /// repository. [`ServiceError::NotFound`] for everyone else and repositories that were
    /// not imported.
    pub async fn status(&self, viewer: &User, repo: &crate::RepoContext) -> Result<RepoImport, ServiceError> {
        let import = self.store().find(repo.repo.repo.id).await?.ok_or(ServiceError::NotFound)?;
        if repo.access.can_admin() || import.importer == Some(viewer.id()) { Ok(import) } else { Err(ServiceError::NotFound) }
    }

    fn spawn(&self, repo: OwnedRepository, importer: User, client: SourceClient, metadata: api::Repo, options: ImportOptions) {
        let service = self.clone();
        self.inner.tasks.spawn(async move {
            let id = repo.repo.id;
            let inner = &service.inner;
            let permit = tokio::select! {
                () = inner.shutdown.cancelled() => None,
                permit = Arc::clone(&inner.permits).acquire_owned() => permit.ok(),
            };
            let error =
                if permit.is_some() { service.run(repo, importer, client, metadata, options).await } else { Some(STOPPED.to_owned()) };
            if let Err(failure) = service.store().finish(id, Timestamp::now(), error.as_deref()).await {
                tracing::error!(error = %failure, repo = id.0, "cannot record the end of an import");
            }
            lock(&inner.running).remove(&id);
        });
    }

    /// Runs one import; returns why it failed, if it did.
    async fn run(
        &self,
        repo: OwnedRepository,
        importer: User,
        client: SourceClient,
        metadata: api::Repo,
        options: ImportOptions,
    ) -> Option<String> {
        let full_name = repo.full_name();
        if let Err(error) = self.store().start(repo.repo.id, Timestamp::now()).await {
            tracing::warn!(%error, repo = %full_name, "cannot mark an import as running");
        }
        let context = match self.inner.repositories.open(Some(&importer), repo.owner.name.as_str(), repo.repo.name.as_str()).await {
            Ok(context) if context.access.can_write() => context,
            _ => return Some("the repository is no longer available to the importing user".to_owned()),
        };
        tracing::info!(repo = %full_name, "import started");
        let services = JobServices { ctx: self.inner.ctx.clone(), lfs: self.inner.lfs.clone(), releases: self.inner.releases.clone() };
        let mut job = Job::new(services, client, metadata, context, importer, options);
        let outcome = tokio::select! {
            () = self.inner.shutdown.cancelled() => Err(STOPPED.to_owned()),
            outcome = job.run() => outcome,
        };
        match &outcome {
            Ok(()) => tracing::info!(repo = %full_name, "import finished"),
            Err(error) => tracing::warn!(repo = %full_name, %error, "import failed"),
        }
        metrics::counter!("rbit_imports_total", "outcome" => if outcome.is_ok() { "ok" } else { "error" }).increment(1);
        outcome.err().map(|error| rbit_core::import::clean_message(&error))
    }

    // ------------------------------------------------------------------ background

    /// Marks imports a restart interrupted as failed. Called once at start-up; later calls
    /// do nothing.
    pub fn spawn_background(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let service = self.clone();
        self.inner.tasks.spawn(async move {
            let running: Vec<RepoId> = lock(&service.inner.running).iter().copied().collect();
            match service.store().fail_unfinished(Timestamp::now(), INTERRUPTED, &running).await {
                Ok(0) => {}
                Ok(count) => tracing::warn!(count, "imports interrupted by a restart were marked failed"),
                Err(error) => tracing::warn!(%error, "cannot mark interrupted imports as failed"),
            }
        });
    }

    /// Stops running imports and waits (bounded) for them to record their end. Called
    /// from [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("imports did not stop within {SHUTDOWN_GRACE:?}");
        }
    }
}

/// The address check for every host an import talks to (see the module docs).
pub(crate) async fn check_source_host(config: &Config, host: &str, port: u16) -> Result<rbit_git::remote::ResolvedHost, String> {
    check_public_host(host, port, config.imports.allow_private_addresses).await
}

/// Locks a mutex, recovering the data if a previous holder panicked (the set stays valid).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
