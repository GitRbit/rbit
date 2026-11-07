//! Static role ports; stores enforce unique races and concrete adapters retain leases/errors.
use crate::{OwnedRepository, Repository, creation::NewRepository};
use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::{Account, User};
use std::{collections::HashSet, future::Future};
pub trait ForkCreationRepository: Sync {
    type Error;
    fn create(&self, new: NewRepository) -> impl Future<Output = Result<Repository, Self::Error>> + Send;
    fn delete(&self, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn mark_pushed(&self, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub trait ForkCreationPort: Sync {
    type Error;
    type CleanupError;
    type Store: ForkCreationRepository;
    fn owner(&self, actor: &User, owner: &str) -> impl Future<Output = Result<Account, Self::Error>> + Send;
    /// Called only after read, owner and name validation, preserving handle acquisition order.
    fn store(&self) -> Self::Store;
    fn copy_disk(&self, source: RepoId, owner: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn record(&self, repo: RepoId, parent: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn delete_disk(&self, owner: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::CleanupError>> + Send;
    fn copy_failed(&self, error: &Self::Error, parent: &OwnedRepository);
    fn cleanup_failed(&self, error: &Self::CleanupError);
    fn pushed_failed(&self, error: &<Self::Store as ForkCreationRepository>::Error);
    fn created(&self, actor: &User, parent: &OwnedRepository, owner: &Account, repo: &Repository);
}
pub trait ForkReadRepository: Sync {
    type Error;
    fn parent_of(&self, repo: RepoId) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send;
    fn forks_among(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashSet<RepoId>, Self::Error>> + Send;
    fn count_readable(
        &self,
        parent: RepoId,
        viewer: Option<AccountId>,
        admin: bool,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn list_readable(
        &self,
        parent: RepoId,
        viewer: Option<AccountId>,
        admin: bool,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
}
pub trait ForkParentAccess: Sync {
    type Error;
    /// False means the original NotFound suppression; other errors retain identity.
    fn can_open(&self, parent: &OwnedRepository) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
