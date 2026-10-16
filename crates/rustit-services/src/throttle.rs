//! Rate limiting for authentication and other abuse-prone endpoints.
//!
//! Milestone 1 workstream S owns this file. The API below is a contract used by other
//! workstreams (email recovery, SSH); keep the signatures stable.

use crate::{ServiceContext, ServiceError};

/// What is being throttled. Each scope has its own limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThrottleScope {
    /// Password sign-in attempts (keyed by client address and by login name).
    SignIn,
    /// Account creation (keyed by client address).
    SignUp,
    /// Second-factor code attempts (keyed by account).
    TwoFactor,
    /// Password-reset and verification emails (keyed by client address and by address).
    EmailRequest,
    /// HTTP Basic credentials on the Git endpoints (keyed by client address and login).
    GitAuth,
    /// SSH authentication attempts (keyed by client address).
    SshAuth,
}

#[derive(Debug, Clone)]
pub struct ThrottleService {
    ctx: ServiceContext,
}

impl ThrottleService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Fails with [`ServiceError::RateLimited`] when `key` has exhausted its budget in `scope`.
    /// Call before doing the expensive or sensitive work.
    pub fn check(&self, scope: ThrottleScope, key: &str) -> Result<(), ServiceError> {
        // TODO(workstream S): enforce limits.
        let _ = (scope, key, &self.ctx);
        Ok(())
    }

    /// Records a failed attempt (wrong password, bad code), which consumes extra budget.
    pub fn record_failure(&self, scope: ThrottleScope, key: &str) {
        let _ = (scope, key);
    }

    /// Clears the failure history after a success (e.g. a correct password).
    pub fn reset(&self, scope: ThrottleScope, key: &str) {
        let _ = (scope, key);
    }
}
