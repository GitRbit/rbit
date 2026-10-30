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
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::Duration,
};

use rbit_auth::{RegistrationSecret, RunnerCredential, TokenDigest};
use rbit_core::{
    Account, AccountKind, Timestamp, User,
    actions::{
        RUNNER_ONLINE_SECONDS, RegistrationToken, RegistrationTokenId, Runner, RunnerBackend, RunnerCapabilities, RunnerId, RunnerLabel,
        RunnerScope, Status, TaskId,
    },
    org::OrgRole,
};
use rbit_storage::{
    StorageError,
    sqlite::actions_runners::{Declaration, NewRegistrationToken, NewRunner, RegistrationRefusal},
};
use rbit_workflow::protocol::{
    Backend, Capabilities, DeclareRequest, DeclareResponse, DeclaredLabel, ErrorBody, ErrorCode, Intervals, MAX_HEARTBEAT_INTERVAL,
    MAX_LONG_POLL, PROTOCOL_VERSION, RegisterRequest, RegisterResponse,
};

use super::ActionsService;
use crate::{
    RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// How long a registration token lasts unless its creator chose otherwise.
pub const DEFAULT_REGISTRATION_LIFETIME: Duration = Duration::from_hours(1);
/// The longest a registration token may last.
pub const MAX_REGISTRATION_LIFETIME: Duration = Duration::from_hours(30 * 24);
/// `last_online` is written at most this often per runner.
const SEEN_GRANULARITY_SECONDS: i64 = 60;
/// Idempotency keys of claims remembered per runner, newest last.
const REMEMBERED_KEYS: usize = 32;

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
    /// The labels its registration token allowed, `None` for any.
    pub pinned_labels: Option<Vec<RunnerLabel>>,
    pub ephemeral: bool,
    pub disabled: bool,
    digest: TokenDigest,
    /// When a call of it last arrived (Unix seconds).
    last_seen: AtomicI64,
    /// The `last_seen` value last written to `last_online`.
    seen_written: AtomicI64,
    /// The `tasks_version` of this runner's last poll that found nothing: while the version
    /// has not moved since, nothing new can be there for it, so a poll waits without a query.
    pub(super) idle_version: AtomicI64,
    /// The idempotency keys of this runner's recent claims and their tasks, so a retried
    /// poll is recognised without a query (see [`super::scheduler`]).
    recent_keys: Mutex<VecDeque<(String, TaskId)>>,
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
            pinned_labels: runner.pinned_labels.clone(),
            ephemeral: runner.ephemeral,
            disabled: runner.disabled,
            digest,
            last_seen: AtomicI64::new(online),
            seen_written: AtomicI64::new(online),
            idle_version: AtomicI64::new(0),
            recent_keys: Mutex::new(VecDeque::new()),
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
        let keys = self.recent_keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.iter().rev().find(|(known, _)| known == key).map(|(_, task)| *task)
    }

    /// Remembers that the claim with `key` gave this runner `task`.
    pub(super) fn remember(&self, key: &str, task: TaskId) {
        let mut keys = self.recent_keys.lock().unwrap_or_else(PoisonError::into_inner);
        if keys.len() == REMEMBERED_KEYS {
            keys.pop_front();
        }
        keys.push_back((key.to_owned(), task));
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
            if let Some(&seen) = seen.get(&runner.id)
                && runner.last_online.is_none_or(|stored| stored.0 < seen)
                && seen > 0
            {
                runner.last_online = Some(Timestamp(seen));
            }
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

/// How a new registration token may be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationOptions {
    /// At most [`MAX_REGISTRATION_LIFETIME`].
    pub lifetime: Duration,
    /// `None`: unlimited.
    pub uses: Option<u32>,
    /// Runners registered with it may declare only these labels.
    pub labels: Option<Vec<RunnerLabel>>,
}

impl Default for RegistrationOptions {
    /// Single-use, for an hour, any labels (§11.9).
    fn default() -> Self {
        Self { lifetime: DEFAULT_REGISTRATION_LIFETIME, uses: Some(1), labels: None }
    }
}

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
        if !self.is_active() {
            return Ok(None);
        }
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
        let long_poll = self.config().long_poll_seconds.clamp(1, MAX_LONG_POLL.as_secs());
        Intervals {
            long_poll_seconds: u32::try_from(long_poll).unwrap_or(1),
            heartbeat_seconds: u32::try_from(MAX_HEARTBEAT_INTERVAL.as_secs()).unwrap_or(10),
        }
    }

    // ------------------------------------------------------------------ the protocol

    /// `POST register`: one use of a registration token for a new runner in its scope. A
    /// token that does not exist, was revoked, expired or is used up is `unauthenticated`
    /// (the caller throttles those); labels outside the token's pin are `forbidden`.
    pub async fn register_runner(&self, request: &RegisterRequest) -> Result<RegisterResponse, RunnerError> {
        request.validate().map_err(RunnerError::from_validation)?;
        let Some(registration) = RegistrationSecret::parse(request.token.expose()) else {
            return Err(invalid_registration());
        };
        let declared = Declared::new(&request.labels, &request.capabilities)?;
        let credential = RunnerCredential::generate();
        let digest = credential.digest();
        let uuid = random_uuid();
        let new = NewRunner {
            registration_digest: &registration.digest(),
            uuid: &uuid,
            token_digest: &digest,
            name: &request.name,
            version: &request.capabilities.version,
            labels: &declared.labels,
            ephemeral: request.capabilities.ephemeral,
            capabilities: &declared.capabilities,
            protocol: PROTOCOL_VERSION,
            now: Timestamp::now(),
        };
        let runner = match self.ctx().database.actions_runners().register(&new).await? {
            Ok(runner) => runner,
            Err(RegistrationRefusal::InvalidToken) => return Err(invalid_registration()),
            Err(RegistrationRefusal::LabelsNotAllowed) => {
                return Err(RunnerError::forbidden("the registration token allows other labels than these"));
            }
        };
        self.runner_cache().await?.insert(&runner, digest);
        if runner.capabilities.runs_on_host() {
            tracing::warn!(runner = runner.id.0, name = %runner.name, "runner registered with host labels: their jobs run without isolation");
        }
        tracing::info!(runner = runner.id.0, name = %runner.name, scope = runner.scope.kind(), "runner registered");
        let target = self.scope_target(runner.scope).await?;
        let entry = AuditEntry::new(AuditAction::ActionsRunnerRegistered).on(target).detail(format!("{} (#{})", runner.name, runner.id));
        self.audit_log().record(entry).await;
        if !runner.labels.is_empty() {
            self.bump_tasks_version();
        }
        Ok(RegisterResponse {
            runner_id: rbit_workflow::protocol::RunnerId(runner.id.0),
            credential: credential.encode().into(),
            intervals: self.intervals(),
        })
    }

    /// `POST declare`: the runner's name, labels and capabilities at its start. Labels
    /// outside its registration token's pin are `forbidden`; a change of labels moves the
    /// scheduler's version, so jobs it can now take are offered to it.
    pub async fn declare_runner(&self, runner: &CachedRunner, request: &DeclareRequest) -> Result<DeclareResponse, RunnerError> {
        request.validate().map_err(RunnerError::from_validation)?;
        let declared = Declared::new(&request.labels, &request.capabilities)?;
        if let Some(pinned) = &runner.pinned_labels
            && let Some(label) = declared.labels.iter().find(|label| !pinned.contains(label))
        {
            return Err(RunnerError::forbidden(format!("this runner's registration token does not allow label `{label}`")));
        }
        let declaration = Declaration {
            name: &request.name,
            labels: &declared.labels,
            version: &request.capabilities.version,
            ephemeral: request.capabilities.ephemeral,
            capabilities: &declared.capabilities,
            protocol: PROTOCOL_VERSION,
            now: Timestamp::now(),
        };
        let Some(stored) = self.ctx().database.actions_runners().declare(runner.id, &declaration).await? else {
            self.runner_cache().await?.remove(runner.id);
            return Err(RunnerError::unauthenticated());
        };
        let mut labels = (declared.labels.clone(), declared.capabilities.host_labels.clone());
        let mut before = (runner.labels.clone(), runner.host_labels.clone());
        for list in [&mut labels.0, &mut labels.1, &mut before.0, &mut before.1] {
            list.sort();
        }
        if labels != before {
            self.bump_tasks_version();
        }
        if let Some(fresh) = self.runner_cache().await?.refresh(&stored) {
            fresh.seen_written.store(declaration.now.0, Ordering::SeqCst);
        }
        if stored.capabilities.runs_on_host() {
            tracing::warn!(runner = stored.id.0, name = %stored.name, "runner declares host labels: their jobs run without isolation");
        }
        Ok(DeclareResponse {
            runner_id: rbit_workflow::protocol::RunnerId(stored.id.0),
            intervals: self.intervals(),
            // Actions: shared cache.
            shared_cache: true,
        })
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
            if seen > written && seen - written >= SEEN_GRANULARITY_SECONDS {
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
        if options.lifetime.is_zero() || options.lifetime > MAX_REGISTRATION_LIFETIME {
            return Err(ServiceError::Invalid("a registration token lasts from a minute up to 30 days".to_owned()));
        }
        if options.uses == Some(0) {
            return Err(ServiceError::Invalid("a registration token must allow at least one registration".to_owned()));
        }
        if options.labels.as_ref().is_some_and(|labels| labels.is_empty() || labels.len() > rbit_core::actions::MAX_RUNNER_LABELS) {
            return Err(ServiceError::Invalid(format!("pin 1 to {} labels, or none to allow any", rbit_core::actions::MAX_RUNNER_LABELS)));
        }
        let secret = RegistrationSecret::generate();
        let now = Timestamp::now();
        let seconds = i64::try_from(options.lifetime.as_secs()).unwrap_or(i64::MAX).max(60);
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
                expires_at: Timestamp(now.0.saturating_add(seconds)),
            })
            .await?;
        let uses = options.uses.map_or_else(|| "unlimited uses".to_owned(), |uses| format!("{uses} use(s)"));
        let detail = format!("#{} until {}, {uses}", token.id, token.expires_at.0);
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
        runners.retain(|runner| runner.scope != RunnerScope::Instance || instance_allowed);
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
        let allowed = match scope {
            ManagedScope::Instance => actor.is_admin,
            ManagedScope::Owner(account) => {
                actor.is_admin
                    || account.id == actor.id()
                    || (account.kind == AccountKind::Organization
                        && self.ctx().database.orgs().role_of(account.id, actor.id()).await? == Some(OrgRole::Owner))
            }
            ManagedScope::Repository(repo) if repo.access.can_admin() => true,
            ManagedScope::Repository(repo) if repo.access.can_read() => return Err(ServiceError::Forbidden),
            ManagedScope::Repository(_) => false,
        };
        if allowed { Ok(()) } else { Err(ServiceError::NotFound) }
    }

    /// The audit target of a stored scope.
    async fn scope_target(&self, scope: RunnerScope) -> Result<String, ServiceError> {
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

    fn audit_log(&self) -> AuditService {
        AuditService::from_store(self.ctx().database.audit())
    }
}

impl RunnerError {
    fn from_validation(error: rbit_workflow::protocol::ValidationError) -> Self {
        Self::bad_request(error.message)
    }
}

fn invalid_registration() -> RunnerError {
    RunnerError::new(ErrorCode::Unauthenticated, "the registration token is unknown, revoked, expired or used up")
}

/// Labels and capabilities as stored, from what a runner sent.
struct Declared {
    labels: Vec<RunnerLabel>,
    capabilities: RunnerCapabilities,
}

impl Declared {
    fn new(labels: &[DeclaredLabel], capabilities: &Capabilities) -> Result<Self, RunnerError> {
        let parsed = labels
            .iter()
            .map(|label| label.name.parse::<RunnerLabel>().map_err(|error| RunnerError::bad_request(error.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        let host_labels =
            labels.iter().zip(&parsed).filter(|(label, _)| label.backend == Backend::Host).map(|(_, parsed)| parsed.clone()).collect();
        let backends = capabilities
            .backends
            .iter()
            .map(|backend| match backend {
                Backend::Container => RunnerBackend::Container,
                Backend::Host => RunnerBackend::Host,
            })
            .collect();
        let capabilities = RunnerCapabilities {
            backends,
            os: capabilities.os.clone(),
            arch: capabilities.arch.clone(),
            capacity: capabilities.capacity,
            host_labels,
        };
        Ok(Self { labels: parsed, capabilities })
    }
}

/// A random (version 4) UUID, as the runner row stores it.
fn random_uuid() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("operating system random number generator is unavailable");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().fold(String::with_capacity(32), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

#[cfg(test)]
mod tests {
    use rbit_core::RepoId;

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
    fn declarations_are_stored_with_their_host_labels() {
        let labels = [
            DeclaredLabel { name: "ubuntu-latest".into(), backend: Backend::Container },
            DeclaredLabel { name: "self-hosted".into(), backend: Backend::Host },
        ];
        let capabilities = Capabilities {
            backends: vec![Backend::Container, Backend::Host],
            os: "linux".into(),
            arch: "x86_64".into(),
            capacity: 2,
            version: "0.2.0".into(),
            ephemeral: false,
        };
        let declared = Declared::new(&labels, &capabilities).unwrap();
        assert_eq!(declared.labels.iter().map(RunnerLabel::as_str).collect::<Vec<_>>(), ["ubuntu-latest", "self-hosted"]);
        assert_eq!(declared.capabilities.host_labels, vec!["self-hosted".parse::<RunnerLabel>().unwrap()]);
        assert_eq!(declared.capabilities.backends, [RunnerBackend::Container, RunnerBackend::Host]);
        assert!(declared.capabilities.runs_on_host());
    }

    #[test]
    fn uuids_are_version_four() {
        let uuid = random_uuid();
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "4");
        assert_ne!(uuid, random_uuid());
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
