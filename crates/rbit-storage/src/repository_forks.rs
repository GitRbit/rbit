//! Role wrappers move existing concrete handles; all fork/repository SQL stays unchanged.
use crate::{RepositoryStore, StorageError, sqlite::forks::ForkStore};
use rbit_core::{AccountId, RepoId};
use rbit_repositories::{
    OwnedRepository, Repository,
    creation::NewRepository,
    forks::{ForkCreationRepository, ForkReadRepository},
};
use std::collections::HashSet;
pub struct ForkCreationStore {
    store: RepositoryStore,
}
impl ForkCreationStore {
    pub fn new(store: RepositoryStore) -> Self {
        Self { store }
    }
}
impl std::fmt::Debug for ForkCreationStore {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ForkCreationStore").finish_non_exhaustive()
    }
}
impl ForkCreationRepository for ForkCreationStore {
    type Error = StorageError;
    fn create(&self, new: NewRepository) -> impl Future<Output = Result<Repository, Self::Error>> + Send {
        self.store.create(new)
    }
    fn delete(&self, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.store.delete(repo)
    }
    fn mark_pushed(&self, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.store.mark_pushed(repo)
    }
}
pub struct ForkReadStore {
    store: ForkStore,
}
impl ForkReadStore {
    pub fn new(store: ForkStore) -> Self {
        Self { store }
    }
}
impl std::fmt::Debug for ForkReadStore {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ForkReadStore").finish_non_exhaustive()
    }
}
impl ForkReadRepository for ForkReadStore {
    type Error = StorageError;
    fn parent_of(&self, repo: RepoId) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send {
        self.store.parent_of(repo)
    }
    fn forks_among(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashSet<RepoId>, Self::Error>> + Send {
        self.store.forks_among(repos)
    }
    fn count_readable(
        &self,
        parent: RepoId,
        viewer: Option<AccountId>,
        admin: bool,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        self.store.count_readable(parent, viewer, admin)
    }
    fn list_readable(
        &self,
        parent: RepoId,
        viewer: Option<AccountId>,
        admin: bool,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.store.list_readable(parent, viewer, admin, limit, offset)
    }
}
