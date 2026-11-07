//! Transferring a repository to another user or organisation.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Rules:
//! * Repository administrators may move a repository to their own account or to an
//!   organisation where they may create repositories (the rule of
//!   `RepositoryService::owner_for_new_repository`); site administrators may move it to
//!   any user or organisation. Suspended users cannot receive repositories.
//! * The full `owner/name` must be typed to confirm. A repository of the same name at the
//!   new owner is a validation error and leaves everything as it was.
//! * The bare repository is moved first (one same-filesystem rename), then the owner is
//!   changed in a transaction that also drops the old organisation's team grants. If the
//!   database step fails a compensating move is attempted. Cancellation or a failed
//!   compensation can leave disk and metadata requiring reconciliation.
//! * Collaborators, stars and everything else keyed by the repository id stay.
//!   [`DomainEvent::RepositoryTransferred`] is published and the transfer is audited.

use rbit_core::{Account, AccountId, RepoId, Repository, User, repository::OwnedRepository};
use rbit_git::relocate::relocate;
use rbit_repositories::transfer::{RepositoryTransfer, TransferError, TransferPort, TransferWriteError};
use rbit_storage::StorageError;

use crate::{
    AuditService, DomainEvent, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    repositories::authorised_owner,
};

#[derive(Debug, Clone)]
pub struct TransferService {
    ctx: ServiceContext,
    audit: AuditService,
}

impl TransferService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { audit: AuditService::new(ctx.clone()), ctx }
    }

    /// Accounts `actor` may move `repo` to without typing a name (their own account and
    /// their organisations), excluding the current owner. Site administrators may also
    /// enter any other name.
    pub async fn suggested_owners(&self, actor: &User, repo: &RepoContext) -> Result<Vec<Account>, ServiceError> {
        require_admin(repo)?;
        let memberships = self.ctx.database.orgs().memberships_of(actor.id()).await?;
        let candidates = std::iter::once(actor.account.clone()).chain(memberships.into_iter().map(|m| m.org));
        Ok(candidates.filter(|account| account.id != repo.repo.repo.owner_id).collect())
    }

    /// Moves `repo` to the account called `new_owner` and returns it as now owned.
    pub async fn transfer(
        &self,
        actor: &User,
        repo: &RepoContext,
        new_owner: &str,
        confirmation: &str,
    ) -> Result<OwnedRepository, ServiceError> {
        RepositoryTransfer::execute(
            &TransferBridge { service: self, repo },
            actor,
            &repo.repo,
            repo.access.can_admin(),
            new_owner,
            confirmation,
        )
        .await
        .map_err(service_error)
    }
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::Forbidden) }
}

// Borrowed request projection; no new store handle exists before target lookup.
#[derive(Debug)]
struct TransferBridge<'a> {
    service: &'a TransferService,
    repo: &'a RepoContext,
}

impl TransferPort for TransferBridge<'_> {
    type Error = ServiceError;
    type MoveError = rbit_git::GitError;

    async fn find_owner(&self, name: &str) -> Result<Option<Account>, Self::Error> {
        Ok(self.service.ctx.database.repositories().find_owner(name).await?)
    }

    async fn authorized_owner(&self, actor: &User, name: &str) -> Result<Account, TransferError<Self::Error>> {
        let store = self.service.ctx.database.repositories();
        authorised_owner(&store, actor, name).await.map_err(|error| match error {
            ServiceError::Invalid(message) => TransferError::Invalid(message),
            other => TransferError::Failure(other),
        })
    }

    async fn is_suspended(&self, account: AccountId) -> Result<bool, Self::Error> {
        Ok(self.service.ctx.database.accounts().is_suspended(account).await?)
    }

    fn close_reader(&self) {
        self.repo.reader.close();
    }

    async fn relocate(&self, repo: RepoId, from: AccountId, to: AccountId) -> Result<(), Self::MoveError> {
        relocate(&self.service.ctx.repositories, repo, from, to).await?;
        Ok(())
    }

    fn move_error(&self, error: Self::MoveError) -> Self::Error {
        error.into()
    }

    async fn transfer(&self, repo: RepoId, owner: AccountId) -> Result<(), TransferWriteError<Self::Error>> {
        self.service.ctx.database.repositories().transfer(repo, owner).await.map_err(|error| match error {
            StorageError::Conflict(_) => TransferWriteError::Conflict,
            other => TransferWriteError::Failure(other.into()),
        })
    }

    fn restore_failed(&self, error: &Self::MoveError, old_name: &str) {
        tracing::error!(restore = %error, repo = %old_name, "failed to move the repository back after a failed transfer");
    }

    async fn transferred(&self, actor: &User, previous: &Repository, moved: &OwnedRepository, old_name: String) {
        tracing::info!(repo = %old_name, to = %moved.full_name(), by = %actor.name(), "repository transferred");
        self.service.ctx.events.publish(DomainEvent::RepositoryTransferred {
            repo: previous.id,
            from: previous.owner_id,
            to: moved.owner.id,
            actor: actor.id(),
        });
        self.service
            .audit
            .record(AuditEntry::new(AuditAction::RepositoryTransferred).by(actor).on(old_name).detail(format!("to {}", moved.full_name())))
            .await;
    }
}

fn service_error(error: TransferError<ServiceError>) -> ServiceError {
    match error {
        TransferError::Forbidden => ServiceError::Forbidden,
        TransferError::Invalid(message) => ServiceError::Invalid(message),
        TransferError::Failure(error) => error,
    }
}
