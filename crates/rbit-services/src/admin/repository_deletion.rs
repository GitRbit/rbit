//! Administrator repository deletion effects around
//! [`rbit_repositories::settings::AdminRepositoryDeletion`].
use rbit_core::{AccountId, OwnedRepository, RepoId, User};
use rbit_repositories::settings::{AdminDeletionError, AdminDeletionPort, AdminRepositoryDeletion};

use super::AdminService;
use crate::{
    DomainEvent, ServiceError,
    audit::{AuditAction, AuditEntry},
};

impl AdminService {
    /// Deletes a repository's metadata and its bare repository on disk.
    /// `confirmation` must repeat `owner/name` exactly.
    pub async fn delete_repository(&self, actor: &User, id: RepoId, confirmation: &str) -> Result<OwnedRepository, ServiceError> {
        let repo = self.repository(actor, id).await?;
        AdminRepositoryDeletion::delete(&DeletionEffects { service: self }, actor, &repo, confirmation).await.map_err(
            |error| match error {
                AdminDeletionError::Invalid(message) => ServiceError::Invalid(message),
                AdminDeletionError::NotFound => ServiceError::NotFound,
                AdminDeletionError::Port(error) => error,
            },
        )?;
        Ok(repo)
    }
}

struct DeletionEffects<'a> {
    service: &'a AdminService,
}

impl AdminDeletionPort for DeletionEffects<'_> {
    type Error = ServiceError;
    type DiskError = rbit_git::GitError;

    async fn delete_metadata(&self, repo: RepoId) -> Result<bool, ServiceError> {
        Ok(self.service.store.delete_repository(repo).await?)
    }

    fn delete_disk(&self, owner: AccountId, repo: RepoId) -> impl Future<Output = Result<(), rbit_git::GitError>> + Send {
        self.service.ctx.repositories.delete(owner, repo)
    }

    /// The database row is already gone, so a failure here leaves only an unreachable
    /// directory behind: log it loudly rather than failing the request.
    fn disk_failed(&self, error: &rbit_git::GitError, owner: AccountId, repo: RepoId) {
        tracing::error!(%error, owner = owner.0, repo = repo.0, "failed to delete repository from disk");
    }

    async fn deleted(&self, actor: &User, repo: &OwnedRepository) {
        tracing::info!(admin = %actor.name(), repository = %repo.full_name(), "repository deleted");
        self.service.ctx.events.publish(DomainEvent::RepositoryDeleted {
            repo: repo.repo.id,
            owner: repo.owner.id,
            full_name: repo.full_name(),
            actor: actor.id(),
        });
        self.service.audit.record(AuditEntry::new(AuditAction::RepositoryDeleted).by(actor).on(repo.full_name())).await;
    }
}
