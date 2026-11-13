//! The smart HTTP access port over the services: the decisions are
//! `rbit_transport::smart_http::access`'s; this adapter only opens repositories.

use rbit_services::{RepoContext, ServiceError};
use rbit_transport::smart_http::access::RepositoryGate;

use super::GitCaller;
use crate::AppState;

/// The [`RepositoryGate`] adapter: repositories as one caller's credential reaches them.
pub(super) struct CallerRepositories<'a> {
    pub(super) state: &'a AppState,
    pub(super) caller: Option<&'a GitCaller>,
}

impl RepositoryGate for CallerRepositories<'_> {
    type Repository = RepoContext;
    type Error = ServiceError;

    /// An old name from before a rename or transfer keeps working, so existing clones do.
    /// A personal access token's scope caps the caller's access.
    async fn open(&self, owner: &str, name: &str) -> Result<Option<RepoContext>, ServiceError> {
        let services = &self.state.services;
        let opened = match (self.caller, &services.actions) {
            // Actions: S. A task opens its own repository at its grant, others only if public.
            (Some(GitCaller::Task(grant)), Some(actions)) => actions.open_for_task(grant, owner, name).await,
            (Some(GitCaller::Task(_)), None) => Err(ServiceError::NotFound),
            (caller, _) => services.repositories.open_following_renames(caller.and_then(GitCaller::user), owner, name).await,
        };
        match opened {
            // Another repository is not there for a limited token, before any other check:
            // a refusal to write it (403) would say that it exists.
            Ok(ctx) if matches!(self.caller, Some(GitCaller::User { token_limit: Some(limit), .. }) if !limit.reaches(ctx.repo.repo.id)) => {
                Ok(None)
            }
            Ok(mut ctx) => {
                if let Some(scope) = self.caller.and_then(GitCaller::token_scope) {
                    ctx.access = scope.limit(ctx.access);
                }
                Ok(Some(ctx))
            }
            Err(ServiceError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn can_write(&self, ctx: &RepoContext) -> bool {
        ctx.access.can_write()
    }

    async fn is_mirror(&self, ctx: &RepoContext) -> Result<bool, ServiceError> {
        self.state.services.mirrors.is_mirror(ctx.repo.repo.id).await
    }
}
