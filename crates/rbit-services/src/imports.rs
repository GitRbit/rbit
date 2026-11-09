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
mod ports;
mod startup;

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_core::{
    Config, RepoId, Timestamp, User, Visibility,
    import::{ImportOptions, RepoImport, SourceKind},
    repository::OwnedRepository,
};
use rbit_storage::sqlite::imports::ImportStore;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use self::client::SourceClient;
use crate::{LfsService, ReleaseService, RepositoryService, ServiceContext, ServiceError, outbound::check_public_host};

/// How long [`ImportService::shutdown`] waits for running imports to record their end.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
use rbit_repositories::imports::{AdmissionError, admit, may_view_status};
pub use rbit_repositories::imports::{INTERRUPTED, STOPPED};

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
    /// Repositories whose import this process runs. The guard also serializes
    /// record admission with the complete restart sweep; never held for Git work.
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
        let admission = ports::Admission { service: self, actor, input: &input };
        admit(&admission, &input.token, &input.name).await.map_err(|error| match error {
            AdmissionError::Invalid(message) => ServiceError::Invalid(message),
            AdmissionError::Failure(error) => error,
        })
    }

    /// The import of `repo` for `viewer`: its importer or an administrator of the
    /// repository. [`ServiceError::NotFound`] for everyone else and repositories that were
    /// not imported.
    pub async fn status(&self, viewer: &User, repo: &crate::RepoContext) -> Result<RepoImport, ServiceError> {
        let import = self.store().find(repo.repo.repo.id).await?.ok_or(ServiceError::NotFound)?;
        if may_view_status(repo.access.can_admin(), import.importer == Some(viewer.id())) {
            Ok(import)
        } else {
            Err(ServiceError::NotFound)
        }
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
            inner.running.lock().await.remove(&id);
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
        let run = ports::Run { service: self, repo, importer, job: std::sync::Mutex::new(Some((client, metadata, options))) };
        rbit_repositories::imports::run(&run).await
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
            match service.fail_interrupted().await {
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
