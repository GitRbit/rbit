//! Branch protection: managing rules, the push gate's decision and merge requirements.
//!
//! Milestone 2 workstream B (branch protection) owns this file.
//!
//! Hooks other code calls (signatures fixed by `docs/design/milestone-2b.md` §3):
//! * [`ProtectionService::check_push`] — the push gate in both receive paths
//!   (`rbit-web/src/routes/git_http.rs`, `rbit-ssh/src/session.rs`), after the access,
//!   mirror and redirect checks and before git starts;
//! * [`ProtectionService::check_merge`] and [`ProtectionService::may_delete_branch`] — P's merge;
//! * [`ProtectionService::protected_branches`] — the branches page's badges.

pub use rbit_core::protection::{MergeRules, PushActor, PushVerdict, Refusal};
use rbit_core::{RefUpdate, User};

use crate::{RepoContext, ServiceContext, ServiceError};

#[derive(Debug, Clone)]
pub struct ProtectionService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl ProtectionService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Decides the ref updates a client asked for, before any object arrives. `repo` is the
    /// resolved repository (never a mirror) and `actor` already has write access.
    /// Stub: everything is allowed.
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "stub until the workstream implements the service")]
    pub async fn check_push(
        &self,
        _repo: &RepoContext,
        _actor: &PushActor<'_>,
        _updates: &[RefUpdate],
    ) -> Result<PushVerdict, ServiceError> {
        Ok(PushVerdict::allow())
    }

    /// What merging into `branch` requires of `merger`. Stub: unprotected.
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "stub until the workstream implements the service")]
    pub async fn check_merge(&self, _repo: &RepoContext, _merger: &User, _branch: &str) -> Result<MergeRules, ServiceError> {
        Ok(MergeRules::unprotected())
    }

    /// Whether `actor` may delete `branch` (P's "delete head branch"). Stub: yes.
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "stub until the workstream implements the service")]
    pub async fn may_delete_branch(&self, _repo: &RepoContext, _actor: &User, _branch: &str) -> Result<bool, ServiceError> {
        Ok(true)
    }

    /// The names of the repository's existing branches that some rule protects.
    /// Stub: none.
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "stub until the workstream implements the service")]
    pub async fn protected_branches(&self, _repo: &RepoContext) -> Result<Vec<String>, ServiceError> {
        Ok(Vec::new())
    }
}
