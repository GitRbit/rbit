//! Site-administrator repository deletion from the admin area.
//!
//! Unlike the settings-page deletion (disk first, then metadata), the administrator path
//! deletes metadata first: once the row is gone the repository is unreachable, and a
//! failed disk removal only leaves an orphaned directory, which is logged rather than
//! failing the request. A row that is already gone is not found and changes nothing.

use std::future::Future;

use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::User;

use crate::OwnedRepository;

/// Metadata deletion, best-effort disk removal and publication.
pub trait AdminDeletionPort: Sync {
    type Error;
    type DiskError;
    /// `false` when the row no longer exists.
    fn delete_metadata(&self, repo: RepoId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn delete_disk(&self, owner: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::DiskError>> + Send;
    fn disk_failed(&self, error: &Self::DiskError, owner: AccountId, repo: RepoId);
    /// Logs, publishes the deletion event, then awaits the best-effort audit.
    fn deleted(&self, actor: &User, repo: &OwnedRepository) -> impl Future<Output = ()> + Send;
}

#[derive(Debug)]
pub enum AdminDeletionError<E> {
    Invalid(String),
    NotFound,
    Port(E),
}

/// One administrator deletion of an already loaded repository; the caller has checked
/// that the actor is a site administrator.
#[derive(Debug)]
pub struct AdminRepositoryDeletion;

impl AdminRepositoryDeletion {
    pub async fn delete<P: AdminDeletionPort>(
        port: &P,
        actor: &User,
        repo: &OwnedRepository,
        confirmation: &str,
    ) -> Result<(), AdminDeletionError<P::Error>> {
        if confirmation.trim() != repo.full_name() {
            return Err(AdminDeletionError::Invalid("type the repository's full name exactly to confirm".to_owned()));
        }
        let id = repo.repo.id;
        if !port.delete_metadata(id).await.map_err(AdminDeletionError::Port)? {
            return Err(AdminDeletionError::NotFound);
        }
        if let Err(error) = port.delete_disk(repo.owner.id, id).await {
            port.disk_failed(&error, repo.owner.id, id);
        }
        port.deleted(actor, repo).await;
        Ok(())
    }
}
