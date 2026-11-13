//! Runners: registration tokens, `register`, `declare` and `unregister`, the in-memory
//! runner cache every protocol call is authenticated against, managing runners from
//! settings, and the errors the runner protocol answers with.
//!
//! * [`RunnerCache`] holds every registered runner by the SHA-256 digest of its credential
//!   (`rbrn_…`, `rbit_auth::RunnerCredential`): authenticating a call is one hash and one
//!   map lookup, with no SQLite access, which is what lets an idle long poll cost nothing.
//!   It is loaded from `actions_runners` on first use and kept current by every change this
//!   module makes to a runner. When a runner was last seen is kept in memory too and written
//!   to `last_online` by the sweeper, at most once a minute ([`ActionsService::flush_seen`]).
//! * Registration tokens (§11.9) expire after an hour and are single-use unless created
//!   otherwise (up to 30 days, unlimited uses). One may pin labels: a runner registered with
//!   it may only ever declare those. Only an administrator creates instance tokens, owners
//!   (a user, an organisation's owners) owner tokens, repository administrators repository
//!   tokens. Whom instance runners serve is the claim query's decision (`actions_owner_policies`).
//! * `register` exchanges a registration token for a runner credential, shown once;
//!   `declare` records labels and capabilities at every start (a label change bumps the
//!   scheduler's version); `unregister` removes the runner. A runner that goes away fails
//!   the tasks it was still running, so their jobs settle at once.
//! * [`RunnerError`] is a protocol answer: an [`ErrorBody`] with its [`ErrorCode`], and a
//!   `Retry-After` for throttled calls. Internal errors are logged here and answered as
//!   `unavailable`, so a runner backs off instead of seeing details.
//!
//! Creating and revoking tokens and registering, disabling, enabling and removing runners
//! are audited.
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
};

pub use rbit_actions::runners::{DEFAULT_REGISTRATION_LIFETIME, MAX_REGISTRATION_LIFETIME};
use rbit_actions::runners::{
    ManagedTarget, Management, RecentKeys, RunnerRefusal, RunnerRegistry, intervals, listed_for_repository, management, seen_is_due,
    shown_online,
};
use rbit_auth::{RegistrationSecret, RunnerCredential, TokenDigest};
use rbit_core::{
    Account, AccountKind, Timestamp, User,
    actions::{RUNNER_ONLINE_SECONDS, RegistrationToken, RegistrationTokenId, Runner, RunnerId, RunnerLabel, RunnerScope, Status, TaskId},
    org::OrgRole,
};
use rbit_storage::{StorageError, sqlite::actions_runners::NewRegistrationToken};
use rbit_workflow::protocol::{DeclareRequest, DeclareResponse, ErrorBody, ErrorCode, Intervals, RegisterRequest, RegisterResponse};

use super::{
    ActionsService,
    runner_ports::{Declaring, Registering},
};
use crate::{
    RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// A runner as protocol calls see it: what authentication, scheduling and the poll need,
/// without a database round trip.
#[derive(Debug)]
pub struct CachedRunner {
    pub id: RunnerId,
    pub name: String,
    pub scope: RunnerScope,
    pub labels: Vec<RunnerLabel>,
    /// Its labels that run jobs on the host, with no isolation: never given untrusted runs.
    pub host_labels: Vec<RunnerLabel>,
    /// Its labels whose jobs need the whole runner.
    pub exclusive_labels: Vec<RunnerLabel>,
    /// The labels its registration token allowed, `None` for any.
    pub pinned_labels: Option<Vec<RunnerLabel>>,
    pub ephemeral: bool,
    pub disabled: bool,
    digest: TokenDigest,
    /// When a call of it last arrived (Unix seconds).
    last_seen: AtomicI64,
    /// The `last_seen` value last written to `last_online`.
    pub(super) seen_written: AtomicI64,
    /// The `tasks_version` of this runner's last poll that found nothing: while the version
    /// has not moved since, nothing new can be there for it, so a poll waits without a query.
    pub(super) idle_version: AtomicI64,
    /// The idempotency keys of this runner's recent claims and their tasks, so a retried
    /// poll is recognised without a query (see [`super::scheduler`]).
    recent_keys: Mutex<RecentKeys<TaskId>>,
    /// Whether this process has looked a key of this runner up in SQLite yet: a retry
    /// across a restart is found there once; claims since are in `recent_keys`.
    pub(super) keys_loaded: AtomicBool,
}

impl CachedRunner {
    fn new(runner: &Runner, digest: TokenDigest) -> Self {
        let online = runner.last_online.map_or(0, |at| at.0);
        Self {
            id: runner.id,
            name: runner.name.clone(),
            scope: runner.scope,
            labels: runner.labels.clone(),
            host_labels: runner.capabilities.host_labels.clone(),
            exclusive_labels: runner.capabilities.exclusive_labels.clone(),
            pinned_labels: runner.pinned_labels.clone(),
            ephemeral: runner.ephemeral,
            disabled: runner.disabled,
            digest,
            last_seen: AtomicI64::new(online),
            seen_written: AtomicI64::new(online),
            idle_version: AtomicI64::new(0),
            recent_keys: Mutex::new(RecentKeys::default()),
            keys_loaded: AtomicBool::new(false),
        }
    }

    /// The same runner with `runner`'s stored fields, keeping what this process learnt.
    fn refreshed(&self, runner: &Runner) -> Self {
        let fresh = Self::new(runner, self.digest);
        fresh.last_seen.store(self.last_seen.load(Ordering::SeqCst).max(fresh.last_seen.load(Ordering::SeqCst)), Ordering::SeqCst);
        fresh.seen_written.store(self.seen_written.load(Ordering::SeqCst), Ordering::SeqCst);
        fresh.idle_version.store(self.idle_version.load(Ordering::SeqCst), Ordering::SeqCst);
        fresh.keys_loaded.store(self.keys_loaded.load(Ordering::SeqCst), Ordering::SeqCst);
        fresh
            .recent_keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone_from(&self.recent_keys.lock().unwrap_or_else(PoisonError::into_inner));
        fresh
    }

    /// Notes a call at `now`; written to the database later, coalesced.
    pub fn seen(&self, now: Timestamp) {
        self.last_seen.fetch_max(now.0, Ordering::SeqCst);
    }

    /// The task this runner claimed with idempotency key `key`, if this process remembers.
    pub(super) fn remembered(&self, key: &str) -> Option<TaskId> {
        self.recent_keys.lock().unwrap_or_else(PoisonError::into_inner).remembered(key)
    }

    /// Remembers that the claim with `key` gave this runner `task`.
    pub(super) fn remember(&self, key: &str, task: TaskId) {
        self.recent_keys.lock().unwrap_or_else(PoisonError::into_inner).remember(key, task);
    }
}

/// Every runner by credential digest. See the module documentation.
#[derive(Debug, Default)]
pub struct RunnerCache {
    by_digest: RwLock<HashMap<TokenDigest, Arc<CachedRunner>>>,
}

impl RunnerCache {
    fn load(runners: Vec<(Runner, TokenDigest)>) -> Self {
        let by_digest = runners.into_iter().map(|(runner, digest)| (digest, Arc::new(CachedRunner::new(&runner, digest)))).collect();
        Self { by_digest: RwLock::new(by_digest) }
    }

    /// The runner whose credential has `digest`.
    pub fn get(&self, digest: &TokenDigest) -> Option<Arc<CachedRunner>> {
        self.by_digest.read().unwrap_or_else(PoisonError::into_inner).get(digest).cloned()
    }

    /// Runner `id`, wherever it is.
    pub fn by_id(&self, id: RunnerId) -> Option<Arc<CachedRunner>> {
        self.by_digest.read().unwrap_or_else(PoisonError::into_inner).values().find(|runner| runner.id == id).cloned()
    }

    /// Adds a newly registered runner.
    pub(super) fn insert(&self, runner: &Runner, digest: TokenDigest) -> Arc<CachedRunner> {
        let cached = Arc::new(CachedRunner::new(runner, digest));
        self.by_digest.write().unwrap_or_else(PoisonError::into_inner).insert(digest, Arc::clone(&cached));
        cached
    }

    /// Replaces runner `runner.id` with its stored state; nothing when it is not cached.
    pub(super) fn refresh(&self, runner: &Runner) -> Option<Arc<CachedRunner>> {
        let mut runners = self.by_digest.write().unwrap_or_else(PoisonError::into_inner);
        let digest = runners.values().find(|cached| cached.id == runner.id)?.digest;
        let fresh = Arc::new(runners[&digest].refreshed(runner));
        runners.insert(digest, Arc::clone(&fresh));
        Some(fresh)
    }

    /// Forgets runner `id`: its credential stops working at once.
    pub(super) fn remove(&self, id: RunnerId) {
        self.by_digest.write().unwrap_or_else(PoisonError::into_inner).retain(|_, runner| runner.id != id);
    }

    /// Every cached runner.
    fn all(&self) -> Vec<Arc<CachedRunner>> {
        self.by_digest.read().unwrap_or_else(PoisonError::into_inner).values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.by_digest.read().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// `runners` with `last_online` brought up to what this process saw last: the stored
    /// value is written at most once a minute, so on its own an idle runner between two
    /// long polls could look offline.
    fn with_seen(&self, mut runners: Vec<Runner>) -> Vec<Runner> {
        let seen: HashMap<RunnerId, i64> = self
            .by_digest
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .map(|runner| (runner.id, runner.last_seen.load(Ordering::SeqCst)))
            .collect();
        for runner in &mut runners {
            runner.last_online = shown_online(runner.last_online.map(|at| at.0), seen.get(&runner.id).copied()).map(Timestamp);
        }
        runners
    }

    // Actions: metrics.
    /// Runners that called within [`RUNNER_ONLINE_SECONDS`] of `now`.
    pub fn online(&self, now: Timestamp) -> usize {
        let since = now.0 - RUNNER_ONLINE_SECONDS;
        self.by_digest
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter(|runner| runner.last_seen.load(Ordering::SeqCst) >= since)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A refused or failed protocol call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerError {
    pub body: ErrorBody,
    /// With [`ErrorCode::Throttled`]: when to try again.
    pub retry_after_seconds: Option<u64>,
}

impl RunnerError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { body: ErrorBody::new(code, message), retry_after_seconds: None }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::BadRequest, message)
    }

    pub fn unauthenticated() -> Self {
        Self::new(ErrorCode::Unauthenticated, "the runner credential is missing, malformed or unknown")
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::TooLarge, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }

    /// A log append that left a gap: the runner resends from `next_line`.
    pub fn log_gap(next_line: u64) -> Self {
        Self { body: ErrorBody::log_gap(next_line), retry_after_seconds: None }
    }

    pub fn throttled(retry_after_seconds: u64) -> Self {
        Self {
            body: ErrorBody::new(ErrorCode::Throttled, format!("too many attempts; try again in {retry_after_seconds} seconds")),
            retry_after_seconds: Some(retry_after_seconds),
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.body.code
    }
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.body.code, self.body.message)
    }
}

impl std::error::Error for RunnerError {}

impl From<ServiceError> for RunnerError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound => Self::not_found("not found"),
            ServiceError::Forbidden => Self::forbidden(error.to_string()),
            ServiceError::Invalid(message) => Self::bad_request(message),
            ServiceError::RateLimited { retry_after_seconds } => Self::throttled(retry_after_seconds),
            other => {
                tracing::error!(error = %other, "runner protocol call failed");
                Self::unavailable("the server could not handle the call; try again later")
            }
        }
    }
}

impl From<StorageError> for RunnerError {
    fn from(error: StorageError) -> Self {
        ServiceError::from(error).into()
    }
}

/// Where runners and registration tokens are managed from, with what the caller already
/// resolved: the owner's account, or the repository as the actor sees it.
#[derive(Debug, Clone, Copy)]
pub enum ManagedScope<'a> {
    /// The administration panel.
    Instance,
    /// A user's or an organisation's settings.
    Owner(&'a Account),
    /// A repository's settings.
    Repository(&'a RepoContext),
}

impl ManagedScope<'_> {
    pub fn scope(&self) -> RunnerScope {
        match self {
            Self::Instance => RunnerScope::Instance,
            Self::Owner(account) => RunnerScope::Owner(account.id),
            Self::Repository(repo) => RunnerScope::Repository(repo.repo.repo.id),
        }
    }

    /// The audit log's target.
    fn target(&self) -> String {
        match self {
            Self::Instance => "instance".to_owned(),
            Self::Owner(account) => account.name.to_string(),
            Self::Repository(repo) => repo.repo.full_name(),
        }
    }
}

/// How a new registration token may be used ([`rbit_actions::runners::RegistrationOptions`]).
pub type RegistrationOptions = rbit_actions::runners::RegistrationOptions<RunnerLabel>;

/// A registration token just created: the stored record and the value, shown once.
#[derive(Debug)]
pub struct IssuedRegistrationToken {
    pub token: RegistrationToken,
    /// `rbrg_…`; never stored.
    pub secret: rbit_workflow::secret::Secret,
}

impl ActionsService {
    /// The runner cache, loaded from the database on first use.
    pub async fn runner_cache(&self) -> Result<&RunnerCache, ServiceError> {
        self.inner
            .runners
            .get_or_try_init(|| async {
                let runners = self.ctx().database.actions_runners().all_with_digests().await?;
                tracing::debug!(runners = runners.len(), "runner cache loaded");
                Ok::<_, ServiceError>(RunnerCache::load(runners))
            })
            .await
    }

    /// The runner a presented credential belongs to: one digest and one cache lookup.
    /// `None` for a malformed or unknown credential; disabled runners are returned, so the
    /// caller decides what they may still do (report on their tasks, not take new ones).
    pub async fn authenticate_runner(&self, credential: &str) -> Result<Option<Arc<CachedRunner>>, ServiceError> {
        let Some(credential) = RunnerCredential::parse(credential) else { return Ok(None) };
        let runner = self.runner_cache().await?.get(&credential.digest());
        if let Some(runner) = &runner {
            runner.seen(Timestamp::now());
        }
        Ok(runner)
    }

    /// The intervals runners are told to keep: `[actions] long_poll_seconds` and the
    /// protocol's heartbeat.
    pub fn intervals(&self) -> Intervals {
        intervals(self.config().long_poll_seconds)
    }

    // ------------------------------------------------------------------ the protocol

    /// `POST register`: one use of a registration token for a new runner in its scope
    /// ([`RunnerRegistry::register`]).
    pub async fn register_runner(&self, request: &RegisterRequest) -> Result<RegisterResponse, RunnerError> {
        RunnerRegistry::register(&Registering::new(self), request).await.map_err(RunnerError::refused)
    }

    /// `POST declare`: the runner's name, labels and capabilities at its start
    /// ([`RunnerRegistry::declare`]).
    pub async fn declare_runner(&self, runner: &CachedRunner, request: &DeclareRequest) -> Result<DeclareResponse, RunnerError> {
        RunnerRegistry::declare(&Declaring::new(self, runner), request).await.map_err(RunnerError::refused)
    }

    /// `POST unregister`: the runner removes itself (an ephemeral runner after its task, or
    /// an uninstall). Tasks it was still running fail.
    pub async fn unregister_runner(&self, runner: &CachedRunner) -> Result<(), RunnerError> {
        self.forget_runner(runner.id, runner.scope).await?;
        tracing::info!(runner = runner.id.0, name = %runner.name, "runner unregistered");
        let target = self.scope_target(runner.scope).await?;
        let entry =
            AuditEntry::new(AuditAction::ActionsRunnerRemoved).on(target).detail(format!("{} (#{}), by itself", runner.name, runner.id));
        self.audit_log().record(entry).await;
        Ok(())
    }

    /// Deletes runner `id` of `scope` and fails its running tasks. False when it is not there.
    async fn forget_runner(&self, id: RunnerId, scope: RunnerScope) -> Result<bool, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let running = store.running_tasks_of(id).await?;
        let deleted = self.ctx().database.actions_runners().delete(scope, id).await?;
        self.runner_cache().await?.remove(id);
        for task in running {
            if let Some(settled) = store.finish_task(task, Status::Failure, Timestamp::now()).await? {
                tracing::info!(task = task.0, runner = id.0, "task failed: its runner was removed");
                self.settled(&settled).await?;
            }
        }
        Ok(deleted)
    }

    /// Writes when runners were last seen, for runners seen since the last write and at
    /// least a minute after it. Called by the sweeper.
    pub async fn flush_seen(&self) -> Result<(), ServiceError> {
        let mut by_time: HashMap<i64, Vec<RunnerId>> = HashMap::new();
        for runner in self.runner_cache().await?.all() {
            let seen = runner.last_seen.load(Ordering::SeqCst);
            let written = runner.seen_written.load(Ordering::SeqCst);
            if seen_is_due(seen, written) {
                runner.seen_written.store(seen, Ordering::SeqCst);
                by_time.entry(seen).or_default().push(runner.id);
            }
        }
        for (seen, ids) in by_time {
            self.ctx().database.actions_runners().touch(&ids, Timestamp(seen)).await?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ settings

    /// Creates a registration token for `scope`, for someone who may manage its runners.
    pub async fn create_registration_token(
        &self,
        actor: &User,
        scope: ManagedScope<'_>,
        options: RegistrationOptions,
    ) -> Result<IssuedRegistrationToken, ServiceError> {
        self.require_manager(actor, scope).await?;
        let now = Timestamp::now();
        let expires_at = options.expires_at(now.0, rbit_core::actions::MAX_RUNNER_LABELS).map_err(ServiceError::Invalid)?;
        let secret = RegistrationSecret::generate();
        let token = self
            .ctx()
            .database
            .actions_runners()
            .create_registration_token(&NewRegistrationToken {
                digest: &secret.digest(),
                scope: scope.scope(),
                labels: options.labels.as_deref(),
                uses: options.uses,
                created_by: Some(actor.id()),
                created_at: now,
                expires_at: Timestamp(expires_at),
            })
            .await?;
        let detail = options.audit_detail(token.id, token.expires_at.0);
        self.audit_log()
            .record(AuditEntry::new(AuditAction::ActionsRegistrationTokenCreated).by(actor).on(scope.target()).detail(detail))
            .await;
        Ok(IssuedRegistrationToken { token, secret: secret.encode().into() })
    }

    /// The registration tokens of exactly `scope`, newest first.
    pub async fn registration_tokens(&self, actor: &User, scope: ManagedScope<'_>) -> Result<Vec<RegistrationToken>, ServiceError> {
        self.require_manager(actor, scope).await?;
        Ok(self.ctx().database.actions_runners().registration_tokens(scope.scope()).await?)
    }

    /// Revokes registration token `id` of `scope`.
    pub async fn revoke_registration_token(
        &self,
        actor: &User,
        scope: ManagedScope<'_>,
        id: RegistrationTokenId,
    ) -> Result<(), ServiceError> {
        self.require_manager(actor, scope).await?;
        if !self.ctx().database.actions_runners().revoke_registration_token(scope.scope(), id, Timestamp::now()).await? {
            return Err(ServiceError::NotFound);
        }
        let entry = AuditEntry::new(AuditAction::ActionsRegistrationTokenRevoked).by(actor).on(scope.target()).detail(format!("#{id}"));
        self.audit_log().record(entry).await;
        Ok(())
    }

    /// The runners of exactly `scope`, oldest first.
    pub async fn runners(&self, actor: &User, scope: ManagedScope<'_>) -> Result<Vec<Runner>, ServiceError> {
        self.require_manager(actor, scope).await?;
        let runners = self.ctx().database.actions_runners().list(scope.scope()).await?;
        Ok(self.runner_cache().await?.with_seen(runners))
    }

    // Actions: REST API.
    /// Whether runner `id` is running a task now (the API's `busy`).
    pub async fn runner_busy(&self, id: RunnerId) -> Result<bool, ServiceError> {
        Ok(!self.ctx().database.actions_runs().running_tasks_of(id).await?.is_empty())
    }

    /// The runners a repository's jobs can run on, as its settings list them: its own, its
    /// owner's and the instance's, the last only when they serve its owner. Read access is
    /// enough to see them.
    pub async fn runners_for_repository(&self, repo: &RepoContext) -> Result<Vec<Runner>, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let owner = repo.repo.owner.id;
        let mut runners = self.ctx().database.actions_runners().list_for_repository(repo.repo.repo.id, owner).await?;
        let instance_allowed =
            self.config().instance_runners_for_all || self.ctx().database.actions_settings().instance_runners_allowed(owner).await?;
        runners.retain(|runner| listed_for_repository(runner.scope == RunnerScope::Instance, instance_allowed));
        Ok(self.runner_cache().await?.with_seen(runners))
    }

    /// Disables or enables runner `id` of `scope`. A disabled runner takes no new jobs; the
    /// tasks it runs may still report and finish.
    pub async fn set_runner_disabled(
        &self,
        actor: &User,
        scope: ManagedScope<'_>,
        id: RunnerId,
        disabled: bool,
    ) -> Result<Runner, ServiceError> {
        self.require_manager(actor, scope).await?;
        let store = self.ctx().database.actions_runners();
        if !store.set_disabled(scope.scope(), id, disabled).await? {
            return Err(ServiceError::NotFound);
        }
        let runner = store.find(id).await?.ok_or(ServiceError::NotFound)?;
        self.runner_cache().await?.refresh(&runner);
        if !disabled {
            self.bump_tasks_version();
        }
        let action = if disabled { AuditAction::ActionsRunnerDisabled } else { AuditAction::ActionsRunnerEnabled };
        let entry = AuditEntry::new(action).by(actor).on(scope.target()).detail(format!("{} (#{})", runner.name, runner.id));
        self.audit_log().record(entry).await;
        Ok(runner)
    }

    /// Removes runner `id` of `scope`: its credential stops working at once and the tasks
    /// it was running fail.
    pub async fn remove_runner(&self, actor: &User, scope: ManagedScope<'_>, id: RunnerId) -> Result<(), ServiceError> {
        self.require_manager(actor, scope).await?;
        let runner = self.ctx().database.actions_runners().find(id).await?.filter(|runner| runner.scope == scope.scope());
        let Some(runner) = runner else { return Err(ServiceError::NotFound) };
        self.forget_runner(id, runner.scope).await?;
        let entry = AuditEntry::new(AuditAction::ActionsRunnerRemoved)
            .by(actor)
            .on(scope.target())
            .detail(format!("{} (#{})", runner.name, runner.id));
        self.audit_log().record(entry).await;
        Ok(())
    }

    /// Who manages runners: administrators the instance's; a user their own, an
    /// organisation's owners (and administrators) the organisation's; repository
    /// administrators the repository's. Others learn nothing (`NotFound`), except readers
    /// of a repository (`Forbidden`).
    async fn require_manager(&self, actor: &User, scope: ManagedScope<'_>) -> Result<(), ServiceError> {
        let target = match scope {
            ManagedScope::Instance => ManagedTarget::Instance,
            ManagedScope::Owner(account) => {
                ManagedTarget::Owner { own_account: account.id == actor.id(), organization: account.kind == AccountKind::Organization }
            }
            ManagedScope::Repository(repo) => ManagedTarget::Repository { admin: repo.access.can_admin(), read: repo.access.can_read() },
        };
        let organization_owner = || async {
            let ManagedScope::Owner(account) = scope else { return Ok(false) };
            Ok::<_, ServiceError>(self.ctx().database.orgs().role_of(account.id, actor.id()).await? == Some(OrgRole::Owner))
        };
        match management(actor.is_admin, target, organization_owner).await? {
            Management::Allowed => Ok(()),
            Management::Forbidden => Err(ServiceError::Forbidden),
            Management::Hidden => Err(ServiceError::NotFound),
        }
    }

    /// The audit target of a stored scope.
    pub(super) async fn scope_target(&self, scope: RunnerScope) -> Result<String, ServiceError> {
        let database = &self.ctx().database;
        Ok(match scope {
            RunnerScope::Instance => "instance".to_owned(),
            RunnerScope::Owner(id) => {
                database.accounts().find_accounts(&[id]).await?.pop().map_or_else(|| format!("#{}", id.0), |a| a.name.to_string())
            }
            RunnerScope::Repository(id) => {
                database.admin().find_repository(id).await?.map_or_else(|| format!("#{}", id.0), |r| r.full_name())
            }
        })
    }

    pub(super) fn audit_log(&self) -> AuditService {
        AuditService::from_store(self.ctx().database.audit())
    }
}

impl RunnerError {
    /// A refused `register` or `declare`, as the protocol answers it.
    fn refused(refusal: RunnerRefusal<Self>) -> Self {
        match refusal {
            RunnerRefusal::BadRequest(message) => Self::bad_request(message),
            RunnerRefusal::Unauthenticated(message) => Self::new(ErrorCode::Unauthenticated, message),
            RunnerRefusal::Forbidden(message) => Self::forbidden(message),
            RunnerRefusal::Port(error) => error,
        }
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{RepoId, actions::RunnerCapabilities};

    use super::*;

    fn runner(id: i64, name: &str) -> Runner {
        Runner {
            id: RunnerId(id),
            uuid: format!("00000000-0000-0000-0000-{id:012}"),
            name: name.to_owned(),
            scope: RunnerScope::Repository(RepoId(1)),
            labels: vec!["linux".parse().unwrap()],
            pinned_labels: None,
            version: "0.2.0".to_owned(),
            ephemeral: false,
            disabled: false,
            last_online: Some(Timestamp(100)),
            created_at: Timestamp(1),
            capabilities: RunnerCapabilities::default(),
            protocol: 1,
        }
    }

    #[test]
    fn listed_runners_show_when_this_process_last_saw_them() {
        let credential = RunnerCredential::generate();
        let cache = RunnerCache::load(vec![(runner(1, "one"), credential.digest())]);
        cache.get(&credential.digest()).unwrap().seen(Timestamp(170));
        let listed = cache.with_seen(vec![runner(1, "one"), runner(2, "unknown")]);
        assert_eq!(listed[0].last_online, Some(Timestamp(170)), "newer than the stored value, which is written once a minute");
        assert_eq!(listed[1].last_online, Some(Timestamp(100)), "a runner this process has not seen keeps the stored value");
        assert!(listed[0].is_online(Timestamp(250)) && !runner(1, "one").is_online(Timestamp(250)));
    }

    #[test]
    fn runners_are_found_by_digest_and_kept_current() {
        let first = RunnerCredential::generate();
        let cache = RunnerCache::load(vec![(runner(1, "one"), first.digest())]);
        assert_eq!(cache.get(&first.digest()).unwrap().name, "one");
        assert!(cache.get(&RunnerCredential::generate().digest()).is_none());

        let cached = cache.get(&first.digest()).unwrap();
        cached.seen(Timestamp(500));
        cached.idle_version.store(42, Ordering::SeqCst);
        cached.remember("key-1", TaskId(3));
        let mut changed = runner(1, "renamed");
        changed.disabled = true;
        let fresh = cache.refresh(&changed).unwrap();
        assert!(fresh.disabled && fresh.name == "renamed");
        assert_eq!(fresh.last_seen.load(Ordering::SeqCst), 500, "what the process learnt survives");
        assert_eq!((fresh.idle_version.load(Ordering::SeqCst), fresh.remembered("key-1")), (42, Some(TaskId(3))));
        assert_eq!(fresh.remembered("key-2"), None);
        for task in 0..40 {
            fresh.remember(&format!("k{task}"), TaskId(task));
        }
        assert_eq!((fresh.remembered("k39"), fresh.remembered("k0")), (Some(TaskId(39)), None), "only the newest are kept");
        assert!(cache.refresh(&runner(9, "unknown")).is_none());

        let second = RunnerCredential::generate();
        cache.insert(&runner(2, "two"), second.digest());
        assert_eq!(cache.by_id(RunnerId(2)).unwrap().name, "two");
        assert_eq!(cache.len(), 2);
        cache.remove(RunnerId(1));
        assert!(cache.get(&first.digest()).is_none());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn errors_map_to_protocol_codes() {
        assert_eq!(RunnerError::from(ServiceError::NotFound).code(), ErrorCode::NotFound);
        assert_eq!(RunnerError::from(ServiceError::Invalid("x".into())).body.message, "x");
        let throttled = RunnerError::from(ServiceError::RateLimited { retry_after_seconds: 7 });
        assert_eq!((throttled.code(), throttled.retry_after_seconds), (ErrorCode::Throttled, Some(7)));
        let internal = RunnerError::from(ServiceError::Internal("disk on fire".into()));
        assert_eq!(internal.code(), ErrorCode::Unavailable);
        assert!(!internal.body.message.contains("disk"), "internal details stay in the log");
        assert_eq!(RunnerError::log_gap(4).body.next_line, Some(4));
    }
}
