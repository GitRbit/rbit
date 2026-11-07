//! Forks: copying a repository to another owner and remembering where it came from.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! # Rules
//!
//! * Anyone who can read a repository may fork it, into their own account or an
//!   organisation where they may create repositories (the same rule as for new
//!   repositories, [`RepositoryService::owner_for_new_repository`]).
//! * The fork keeps the parent's visibility (a private parent yields a private fork),
//!   description and default branch; its branches and tags are copied on disk with
//!   `git clone --bare --local` (see [`rbit_git::fork`]).
//! * A fork cannot be made public while its parent, or any repository further up the
//!   fork network, is private ([`has_private_ancestor`]): that would publish the private
//!   history. (Forks are not removed when someone loses access to the parent.)
//! * Metadata precedes copying. Completed copy/relationship errors attempt disk and
//!   SQL rollback; cancellation or failed cleanup can still require repair.
//! * Forking publishes [`crate::DomainEvent::RepositoryForked`] — not `RepositoryCreated`.
//! * Deleting the parent leaves its forks intact; they simply lose their parent.

use crate::{EventBus, RepoContext, RepositoryService, ServiceContext, ServiceError};
use rbit_core::{Config, RepoId, User, repository::OwnedRepository};
use rbit_git::RepositoryStorage;
use rbit_repositories::forks::{ForkCreation, ForkCreationError, ForkParentError, ForkQueries};
pub use rbit_repositories::forks::{ForkInput, MAX_LISTED_FORKS};
use rbit_storage::{Database, ForkReadStore};
use std::{collections::HashSet, sync::Arc};
mod effects;
mod reading;
use effects::ForkEffects;
use reading::ParentAccess;

#[derive(Debug, Clone)]
pub struct ForkService {
    config: Arc<Config>,
    database: Database,
    storage: RepositoryStorage,
    events: EventBus,
    repositories: RepositoryService,
}
impl ForkService {
    pub(crate) fn new(ctx: ServiceContext, repositories: RepositoryService) -> Self {
        Self { config: ctx.config, database: ctx.database, storage: ctx.repositories, events: ctx.events, repositories }
    }
    pub async fn fork(&self, actor: &User, parent: &RepoContext, input: ForkInput) -> Result<OwnedRepository, ServiceError> {
        let effects = ForkEffects {
            config: &self.config,
            database: &self.database,
            storage: &self.storage,
            events: &self.events,
            owners: &self.repositories,
            parent,
        };
        ForkCreation::new(&effects).fork(actor, &parent.repo, parent.access.can_read(), input).await.map_err(|error| match error {
            ForkCreationError::NotFound => ServiceError::NotFound,
            ForkCreationError::Name(error) => error.into(),
            ForkCreationError::Owner(error) | ForkCreationError::Copy(error) => error,
            ForkCreationError::Store(error) => error.into(),
        })
    }
    pub async fn parent(&self, viewer: Option<&User>, repo: &RepoContext) -> Result<Option<OwnedRepository>, ServiceError> {
        let store = ForkReadStore::new(self.database.forks());
        let access = ParentAccess { repositories: &self.repositories, viewer };
        ForkQueries::new(&store).parent(repo.repo.repo.id, &access).await.map_err(|error| match error {
            ForkParentError::Store(error) => error.into(),
            ForkParentError::Access(error) => error,
        })
    }
    pub async fn forks_among(&self, repos: &[RepoId]) -> Result<HashSet<RepoId>, ServiceError> {
        Ok(ForkQueries::new(&ForkReadStore::new(self.database.forks())).forks_among(repos).await?)
    }
    pub async fn is_fork(&self, repo: RepoId) -> Result<bool, ServiceError> {
        Ok(ForkQueries::new(&ForkReadStore::new(self.database.forks())).is_fork(repo).await?)
    }
    pub async fn count(&self, viewer: Option<&User>, repo: &RepoContext) -> Result<u64, ServiceError> {
        Ok(ForkQueries::new(&ForkReadStore::new(self.database.forks())).count(reading::viewer(viewer), repo.repo.repo.id).await?)
    }
    pub async fn list(&self, viewer: Option<&User>, repo: &RepoContext) -> Result<Vec<OwnedRepository>, ServiceError> {
        let viewer = reading::viewer(viewer);
        let store = ForkReadStore::new(self.database.forks());
        Ok(ForkQueries::new(&store).list(viewer, repo.repo.repo.id).await?)
    }
    pub async fn page(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<OwnedRepository>, u64), ServiceError> {
        let viewer = reading::viewer(viewer);
        let store = ForkReadStore::new(self.database.forks());
        Ok(ForkQueries::new(&store).page(viewer, repo.repo.repo.id, limit, offset).await?)
    }
}
pub(crate) async fn has_private_ancestor(database: &Database, repo: RepoId) -> Result<bool, ServiceError> {
    Ok(ForkQueries::new(&ForkReadStore::new(database.forks())).has_private_ancestor(repo).await?)
}
