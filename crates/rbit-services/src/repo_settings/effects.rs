//! Concrete Git, existing stores and cross-feature publication; no service context retention.
use crate::{
    AuditService, DomainEvent, EventBus, RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry},
};
use rbit_core::User;
use rbit_git::{RefKind, RepositoryStorage};
use rbit_repositories::{
    RepoName,
    settings::{RepoGeneralSettings, RepositorySettingsPort},
};
use rbit_storage::{Database, StorageError, sqlite::repo_settings::RepoSettingsStore};
pub(super) struct SettingsEffects<'a> {
    pub database: &'a Database,
    pub storage: &'a RepositoryStorage,
    pub events: &'a EventBus,
    pub audit: &'a AuditService,
    pub repo: &'a RepoContext,
}
impl RepositorySettingsPort for SettingsEffects<'_> {
    type Error = ServiceError;
    type StoreError = StorageError;
    type Store = RepoSettingsStore;
    fn store(&self) -> Self::Store {
        self.database.repo_settings()
    }
    async fn has_private_ancestor(&self) -> Result<bool, Self::Error> {
        crate::forks::has_private_ancestor(self.database, self.repo.repo.repo.id).await
    }
    async fn branch_exists(&self, branch: &str) -> Result<bool, Self::Error> {
        let refs = self.repo.reader.refs().await?;
        Ok(refs.iter().any(|reference| reference.kind == RefKind::Branch && reference.name == branch))
    }
    async fn set_head(&self, branch: &str) -> Result<(), Self::Error> {
        let current = &self.repo.repo.repo;
        Ok(self.storage.set_head(current.owner_id, current.id, branch).await?)
    }
    fn restore_failed(&self, error: &Self::Error) {
        match error {
            ServiceError::Internal(error) => {
                tracing::error!(restore=%error,repo=%self.repo.repo.full_name(),"failed to restore HEAD after a failed settings update");
            }
            error => {
                tracing::error!(restore=%error,repo=%self.repo.repo.full_name(),"failed to restore HEAD after a failed settings update");
            }
        }
    }
    fn updated(&self, settings: &RepoGeneralSettings) {
        tracing::info!(repo=%self.repo.repo.full_name(),visibility=settings.visibility.as_str(),default_branch=%settings.default_branch,"repository settings updated");
    }
    fn renamed(&self, name: &RepoName) {
        tracing::info!(repo=%self.repo.repo.full_name(),new_name=%name,"repository renamed");
    }
    async fn archive_committed(&self, actor: &User, archived: bool) {
        let full_name = self.repo.repo.full_name();
        let action = if archived { AuditAction::RepositoryArchived } else { AuditAction::RepositoryUnarchived };
        tracing::info!(repo=%full_name,archived,by=%actor.name(),"repository archive state changed");
        self.audit.record(AuditEntry::new(action).by(actor).on(full_name)).await;
        self.events.publish(DomainEvent::RepositoryArchived { repo: self.repo.repo.repo.id, archived, actor: actor.id() });
    }
    fn close_reader(&self) {
        self.repo.reader.close();
    }
    async fn delete_disk(&self) -> Result<(), Self::Error> {
        let current = &self.repo.repo.repo;
        Ok(self.storage.delete(current.owner_id, current.id).await?)
    }
    fn deleted(&self, actor: &User, full_name: String) {
        tracing::info!(repo=%full_name,"repository deleted");
        let current = &self.repo.repo.repo;
        self.events.publish(DomainEvent::RepositoryDeleted { repo: current.id, owner: current.owner_id, full_name, actor: actor.id() });
    }
}
