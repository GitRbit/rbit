//! Runners: the in-memory runner cache every protocol call is authenticated against, and
//! the errors the runner protocol answers with.
//!
//! * [`RunnerCache`] holds every registered runner by the SHA-256 digest of its credential
//!   (`rbrn_…`, `rbit_auth::RunnerCredential`): authenticating a call is one hash and one
//!   map lookup, with no SQLite access, which is what lets an idle long poll cost nothing.
//!   It is loaded from `actions_runners` on first use.
//! * [`RunnerError`] is a protocol answer: an [`ErrorBody`] with its [`ErrorCode`], and a
//!   `Retry-After` for throttled calls. Internal errors are logged here and answered as
//!   `unavailable`, so a runner backs off instead of seeing details.
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::{
    collections::HashMap,
    sync::{Arc, PoisonError, RwLock},
};

use rbit_auth::{RunnerCredential, TokenDigest};
use rbit_core::actions::{Runner, RunnerId, RunnerLabel, RunnerScope};
use rbit_storage::StorageError;
use rbit_workflow::protocol::{ErrorBody, ErrorCode};

use super::ActionsService;
use crate::ServiceError;

/// A runner as protocol calls see it: what authentication, scheduling and the poll need,
/// without a database round trip.
#[derive(Debug)]
pub struct CachedRunner {
    pub id: RunnerId,
    pub name: String,
    pub scope: RunnerScope,
    pub labels: Vec<RunnerLabel>,
    /// The labels its registration token allowed, `None` for any.
    pub pinned_labels: Option<Vec<RunnerLabel>>,
    pub ephemeral: bool,
    pub disabled: bool,
}

impl CachedRunner {
    fn new(runner: &Runner) -> Self {
        Self {
            id: runner.id,
            name: runner.name.clone(),
            scope: runner.scope,
            labels: runner.labels.clone(),
            pinned_labels: runner.pinned_labels.clone(),
            ephemeral: runner.ephemeral,
            disabled: runner.disabled,
        }
    }
}

/// Every runner by credential digest. See the module documentation.
#[derive(Debug, Default)]
pub struct RunnerCache {
    by_digest: RwLock<HashMap<TokenDigest, Arc<CachedRunner>>>,
}

impl RunnerCache {
    fn load(runners: Vec<(Runner, TokenDigest)>) -> Self {
        let by_digest = runners.into_iter().map(|(runner, digest)| (digest, Arc::new(CachedRunner::new(&runner)))).collect();
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

    pub fn len(&self) -> usize {
        self.by_digest.read().unwrap_or_else(PoisonError::into_inner).len()
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
    /// caller can say why they are refused.
    pub async fn authenticate_runner(&self, credential: &str) -> Result<Option<Arc<CachedRunner>>, ServiceError> {
        let Some(credential) = RunnerCredential::parse(credential) else { return Ok(None) };
        Ok(self.runner_cache().await?.get(&credential.digest()))
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{RepoId, Timestamp};

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
        }
    }

    #[test]
    fn runners_are_found_by_digest() {
        let first = RunnerCredential::generate();
        let cache = RunnerCache::load(vec![(runner(1, "one"), first.digest())]);
        assert_eq!(cache.get(&first.digest()).unwrap().name, "one");
        assert_eq!(cache.by_id(RunnerId(1)).unwrap().name, "one");
        assert!(cache.get(&RunnerCredential::generate().digest()).is_none());
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
