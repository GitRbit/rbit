//! Repository stars.
//!
//! Workstream D owns this file. Starring takes a [`RepoContext`] from
//! [`crate::RepositoryService::open`], so only repositories the user can read can be starred.

use std::collections::HashMap;

use rbit_core::{Account, RepoId, User, repository::OwnedRepository};
use rbit_storage::sqlite::stars::StarStore;

pub use rbit_storage::sqlite::stars::Stargazer;

use crate::{RepoContext, ServiceContext, ServiceError};

/// Lists show at most this many entries (newest first).
const MAX_LISTED: u32 = 500;

/// Star state of a repository as seen by one viewer.
#[derive(Debug, Clone, Copy, Default)]
pub struct StarSummary {
    pub count: i64,
    pub starred_by_viewer: bool,
}

#[derive(Debug, Clone)]
pub struct StarService {
    ctx: ServiceContext,
}

impl StarService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    fn store(&self) -> StarStore {
        self.ctx.database.stars()
    }

    /// Used by every repository page header. Contract: must stay cheap (one indexed query).
    pub async fn summary(&self, repo: RepoId, viewer: Option<&User>) -> Result<StarSummary, ServiceError> {
        let counted = self.store().count(repo, viewer.map(User::id)).await?;
        Ok(StarSummary { count: counted.count, starred_by_viewer: counted.starred })
    }

    /// Star counts for a listing of repositories (one query). Unstarred repositories are absent.
    pub async fn counts(&self, repos: &[OwnedRepository]) -> Result<HashMap<RepoId, i64>, ServiceError> {
        let ids: Vec<RepoId> = repos.iter().map(|r| r.repo.id).collect();
        Ok(self.store().counts(&ids).await?)
    }

    /// Stars `repo` for `user` (idempotent) and returns the new summary.
    pub async fn star(&self, user: &User, repo: &RepoContext) -> Result<StarSummary, ServiceError> {
        require_read(repo)?;
        self.store().add(user.id(), repo.repo.repo.id).await?;
        self.summary(repo.repo.repo.id, Some(user)).await
    }

    /// Removes `user`'s star (idempotent) and returns the new summary.
    pub async fn unstar(&self, user: &User, repo: &RepoContext) -> Result<StarSummary, ServiceError> {
        require_read(repo)?;
        self.store().remove(user.id(), repo.repo.repo.id).await?;
        self.summary(repo.repo.repo.id, Some(user)).await
    }

    /// Accounts that starred `repo`, most recent first (at most 500).
    pub async fn stargazers(&self, repo: &RepoContext) -> Result<Vec<Stargazer>, ServiceError> {
        require_read(repo)?;
        Ok(self.store().stargazers(repo.repo.repo.id, MAX_LISTED).await?)
    }

    /// Repositories `account` starred that `viewer` may see, most recently starred first (at most 500).
    pub async fn starred_by(&self, viewer: Option<&User>, account: &Account) -> Result<Vec<OwnedRepository>, ServiceError> {
        let is_admin = viewer.is_some_and(|v| v.is_admin);
        Ok(self.store().starred_by(account.id, viewer.map(User::id), is_admin, MAX_LISTED).await?)
    }
}

fn require_read(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_read() { Ok(()) } else { Err(ServiceError::NotFound) }
}
