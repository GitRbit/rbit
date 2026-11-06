//! Static catalog, persistence and directory/event ports; SQL remains the unique-race authority.
use super::NewRepository;
use crate::Repository;
use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::{Account, User};
use std::future::Future;
/// Minimal membership fact required by creation, not a replacement organization wire role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryMembership {
    Member,
    Owner,
}
pub trait RepositoryOwnerCatalog: Sync {
    type Error;
    fn owner_named(&self, name: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send;
    fn membership(
        &self,
        owner: AccountId,
        actor: AccountId,
    ) -> impl Future<Output = Result<Option<RepositoryMembership>, Self::Error>> + Send;
}
pub trait RepositoryCreationRepository: RepositoryOwnerCatalog {
    fn create(&self, new: NewRepository) -> impl Future<Output = Result<Repository, Self::Error>> + Send;
    fn delete(&self, id: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn grant_creator_admin(&self, id: RepoId, actor: AccountId) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub trait RepositoryCreationPort: Sync {
    type Error;
    type Store: RepositoryCreationRepository;
    fn repository(&self) -> &Self::Store;
    fn default_branch(&self) -> &str;
    fn create_disk(&self, owner: AccountId, repo: RepoId, branch: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn disk_failed(&self, error: &Self::Error, repo: &Repository);
    fn created(&self, actor: &User, owner: &Account, repo: &Repository);
}
