//! Concrete leased Git copy, SQL edge and original diagnostics/event effects.
use crate::{DomainEvent, EventBus, RepoContext, RepositoryService, ServiceError};
use rbit_core::{Account, AccountId, Config, RepoId, Repository, User, repository::OwnedRepository};
use rbit_git::{GitError, RepositoryStorage, fork::fork_repository};
use rbit_repositories::forks::ForkCreationPort;
use rbit_storage::{Database, ForkCreationStore, StorageError};
pub(super) struct ForkEffects<'a> {
    pub config: &'a Config,
    pub database: &'a Database,
    pub storage: &'a RepositoryStorage,
    pub events: &'a EventBus,
    pub owners: &'a RepositoryService,
    pub parent: &'a RepoContext,
}
impl ForkCreationPort for ForkEffects<'_> {
    type Error = ServiceError;
    type CleanupError = GitError;
    type Store = ForkCreationStore;
    fn owner(&self, actor: &User, owner: &str) -> impl Future<Output = Result<Account, Self::Error>> + Send {
        self.owners.owner_for_new_repository(actor, owner)
    }
    fn store(&self) -> Self::Store {
        ForkCreationStore::new(self.database.repositories())
    }
    async fn copy_disk(&self, source: RepoId, owner: AccountId, repo: RepoId) -> Result<(), Self::Error> {
        let target = self.storage.path_for(owner, repo);
        let lease = self.storage.lease(source).await;
        fork_repository(&self.config.repository.git_binary, &self.parent.disk_path, &target).await?;
        drop(lease);
        Ok(())
    }
    async fn record(&self, repo: RepoId, parent: RepoId) -> Result<(), Self::Error> {
        Ok(self.database.forks().record(repo, parent).await?)
    }
    fn delete_disk(&self, owner: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::CleanupError>> + Send {
        self.storage.delete(owner, repo)
    }
    fn copy_failed(&self, error: &Self::Error, parent: &OwnedRepository) {
        tracing::error!(%error,parent=%parent.full_name(),"forking failed; rolling back");
    }
    fn cleanup_failed(&self, error: &GitError) {
        tracing::error!(%error,"cannot remove the partial fork from disk");
    }
    fn pushed_failed(&self, error: &StorageError) {
        tracing::warn!(%error,"cannot record the fork's content");
    }
    fn created(&self, actor: &User, parent: &OwnedRepository, owner: &Account, repo: &Repository) {
        tracing::info!(actor=%actor.name(),parent=%parent.full_name(),fork=%format!("{}/{}",owner.name,repo.name),"repository forked");
        self.events.publish(DomainEvent::RepositoryForked { repo: repo.id, parent: parent.repo.id, actor: actor.id() });
    }
}
