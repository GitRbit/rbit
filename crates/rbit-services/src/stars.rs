//! Repository stars: composition over [`rbit_repositories::stars::RepositoryStars`],
//! which owns the read gate, idempotent commands and listing bounds. Starring takes a
//! [`RepoContext`] from [`crate::RepositoryService::open`]; its read access is the
//! authority the application checks.

use std::collections::HashMap;

use rbit_core::{Account, RepoId, User, repository::OwnedRepository};
use rbit_repositories::stars::{RepositoryStars, StarError};

pub use rbit_repositories::stars::{StarSummary, Stargazer};

use crate::{RepoContext, ServiceContext, ServiceError};

#[derive(Debug, Clone)]
pub struct StarService {
    ctx: ServiceContext,
}

impl StarService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Used by every repository page header. Contract: must stay cheap (one indexed query).
    pub async fn summary(&self, repo: RepoId, viewer: Option<&User>) -> Result<StarSummary, ServiceError> {
        Ok(RepositoryStars::new(&self.ctx.database.stars()).summary(repo, viewer).await?)
    }

    /// Star counts for a listing of repositories (one query). Unstarred repositories are absent.
    pub async fn counts(&self, repos: &[OwnedRepository]) -> Result<HashMap<RepoId, i64>, ServiceError> {
        Ok(RepositoryStars::new(&self.ctx.database.stars()).counts(repos).await?)
    }

    /// Stars `repo` for `user` (idempotent) and returns the new summary.
    pub async fn star(&self, user: &User, repo: &RepoContext) -> Result<StarSummary, ServiceError> {
        RepositoryStars::new(&self.ctx.database.stars()).star(user, &repo.repo, repo.access.can_read()).await.map_err(star_error)
    }

    /// Removes `user`'s star (idempotent) and returns the new summary.
    pub async fn unstar(&self, user: &User, repo: &RepoContext) -> Result<StarSummary, ServiceError> {
        RepositoryStars::new(&self.ctx.database.stars()).unstar(user, &repo.repo, repo.access.can_read()).await.map_err(star_error)
    }

    /// Accounts that starred `repo`, most recent first (at most 500).
    pub async fn stargazers(&self, repo: &RepoContext) -> Result<Vec<Stargazer>, ServiceError> {
        RepositoryStars::new(&self.ctx.database.stars()).stargazers(&repo.repo, repo.access.can_read()).await.map_err(star_error)
    }

    /// Repositories `account` starred that `viewer` may see, most recently starred first (at most 500).
    pub async fn starred_by(&self, viewer: Option<&User>, account: &Account) -> Result<Vec<OwnedRepository>, ServiceError> {
        Ok(RepositoryStars::new(&self.ctx.database.stars()).starred_by(viewer, account).await?)
    }
}

fn star_error(error: StarError<rbit_storage::StorageError>) -> ServiceError {
    match error {
        StarError::NotFound => ServiceError::NotFound,
        StarError::Store(error) => error.into(),
    }
}
