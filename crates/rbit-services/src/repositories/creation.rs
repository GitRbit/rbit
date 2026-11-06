//! Original public creation API composes static feature authority with concrete Git/stores/events.
use super::{CreateRepository, RepositoryService};
use crate::{DomainEvent, ServiceError};
use rbit_core::{Account, AccountId, RepoId, Repository, User};
use rbit_git::GitError;
use rbit_repositories::{
    OwnedRepository,
    creation::{RepositoryCreation, RepositoryCreationError, RepositoryCreationPort, RepositoryOwnerError},
};
use rbit_storage::{RepositoryStore, StorageError};
struct CreationEffects<'a> {
    service: &'a RepositoryService,
}
impl RepositoryCreationPort for CreationEffects<'_> {
    type Error = GitError;
    type Store = RepositoryStore;
    fn repository(&self) -> &Self::Store {
        &self.service.store
    }
    fn default_branch(&self) -> &str {
        &self.service.config.repository.default_branch
    }
    async fn create_disk(&self, owner: AccountId, repo: RepoId, branch: &str) -> Result<(), Self::Error> {
        self.service.storage.create(owner, repo, branch).await.map(|_| ())
    }
    fn disk_failed(&self, error: &Self::Error, repo: &Repository) {
        tracing::error!(%error,repo=%repo.name,"failed to initialise repository on disk; rolling back");
    }
    fn created(&self, actor: &User, owner: &Account, repo: &Repository) {
        tracing::info!(actor=%actor.name(),owner=%owner.name,repo=%repo.name,"repository created");
        self.service.events.publish(DomainEvent::RepositoryCreated { repo: repo.id, actor: actor.id() });
    }
}
pub(super) fn owner_error(error: RepositoryOwnerError<StorageError>) -> ServiceError {
    match error {
        RepositoryOwnerError::Port(error) => error.into(),
        RepositoryOwnerError::Invalid(message) => ServiceError::Invalid(message),
    }
}
impl RepositoryService {
    /// Personally owned creation retains the original owner-name input and fast path.
    pub async fn create(&self, actor: &User, input: CreateRepository) -> Result<OwnedRepository, ServiceError> {
        self.create_in(actor, actor.name().as_str(), input).await
    }
    /// Metadata, bare Git initialization, optional creator grant and publication retain their original order.
    pub async fn create_in(&self, actor: &User, owner: &str, input: CreateRepository) -> Result<OwnedRepository, ServiceError> {
        RepositoryCreation::new(&CreationEffects { service: self }).create_in(actor, owner, input).await.map_err(|error| match error {
            RepositoryCreationError::Owner(error) => owner_error(error),
            RepositoryCreationError::Name(error) => error.into(),
            RepositoryCreationError::Invalid(message) => ServiceError::Invalid(message),
            RepositoryCreationError::Store(error) => error.into(),
            RepositoryCreationError::Disk(error) => error.into(),
        })
    }
}
