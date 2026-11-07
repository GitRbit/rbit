//! Existing catalog and creation SQL remain authoritative; project only the needed membership fact.
use crate::{RepositoryStore, StorageError};
use rbit_core::{Account, AccountId, OrgRole, Permission, RepoId};
use rbit_repositories::{
    Repository,
    creation::{NewRepository, RepositoryCreationRepository, RepositoryMembership, RepositoryOwnerCatalog},
};
use std::future::Future;
impl RepositoryOwnerCatalog for RepositoryStore {
    type Error = StorageError;
    fn owner_named(&self, name: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send {
        Self::find_owner(self, name)
    }
    async fn membership(&self, owner: AccountId, actor: AccountId) -> Result<Option<RepositoryMembership>, Self::Error> {
        Ok(self.org_role(owner, actor).await?.map(|role| match role {
            OrgRole::Member => RepositoryMembership::Member,
            OrgRole::Owner => RepositoryMembership::Owner,
        }))
    }
}
impl RepositoryCreationRepository for RepositoryStore {
    fn create(&self, new: NewRepository) -> impl Future<Output = Result<Repository, Self::Error>> + Send {
        Self::create(self, new)
    }
    fn delete(&self, id: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        Self::delete(self, id)
    }
    fn grant_creator_admin(&self, id: RepoId, actor: AccountId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.grant_collaborator(id, actor, Permission::Admin)
    }
}
