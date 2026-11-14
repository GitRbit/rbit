//! Fork composition: native Git copy, relationship recording and events around
//! [`rbit_repositories::forks`], which owns the fork rules and visibility queries.
//!
//! Every fork-store read in this crate goes through [`ForkQueries`] here: the
//! [`ForkService`] methods and the crate helpers `forks_among`, `is_fork`,
//! `network` and `has_private_ancestor`.

use crate::{EventBus, ProtectionService, PushService, RepoContext, RepositoryService, ServiceContext, ServiceError};
use rbit_core::{Config, RepoId, User, repository::OwnedRepository};
use rbit_git::RepositoryStorage;
use rbit_repositories::forks::{ForkCreation, ForkCreationError, ForkParentError, ForkQueries};
pub use rbit_repositories::forks::{ForkInput, MAX_LISTED_FORKS};
use rbit_storage::{Database, ForkReadStore};
use std::{collections::HashSet, sync::Arc};
mod effects;
mod reading;
mod sync;
use effects::ForkEffects;
use reading::ParentAccess;
pub use sync::{ForkSync, ForkSyncExpectation};

#[derive(Debug, Clone)]
pub struct ForkService {
    config: Arc<Config>,
    database: Database,
    storage: RepositoryStorage,
    events: EventBus,
    repositories: RepositoryService,
    protection: ProtectionService,
    pushes: PushService,
}
impl ForkService {
    pub(crate) fn new(ctx: ServiceContext, repositories: RepositoryService) -> Self {
        Self {
            protection: ProtectionService::new(ctx.clone()),
            pushes: PushService::new(ctx.clone()),
            config: ctx.config,
            database: ctx.database,
            storage: ctx.repositories,
            events: ctx.events,
            repositories,
        }
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
        forks_among(&self.database, repos).await
    }
    pub async fn is_fork(&self, repo: RepoId) -> Result<bool, ServiceError> {
        is_fork(&self.database, repo).await
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
/// Which of `repos` are forks; one query, none for an empty slice.
pub(crate) async fn forks_among(database: &Database, repos: &[RepoId]) -> Result<HashSet<RepoId>, ServiceError> {
    Ok(ForkQueries::new(&ForkReadStore::new(database.forks())).forks_among(repos).await?)
}
pub(crate) async fn is_fork(database: &Database, repo: RepoId) -> Result<bool, ServiceError> {
    Ok(ForkQueries::new(&ForkReadStore::new(database.forks())).is_fork(repo).await?)
}
/// The readable fork network of `repo` for `viewer`, `repo` included when readable.
pub(crate) async fn network(database: &Database, viewer: Option<&User>, repo: RepoId) -> Result<Vec<OwnedRepository>, ServiceError> {
    Ok(ForkQueries::new(&ForkReadStore::new(database.forks())).network(reading::viewer(viewer), repo).await?)
}
