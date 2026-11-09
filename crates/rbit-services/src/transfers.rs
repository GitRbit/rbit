//! Repository transfer composition: the request's reader, disk relocation, events and
//! audit around [`rbit_repositories::transfer::RepositoryTransfer`].
//!
//! The application owns administration, confirmation, destination authority,
//! suspension, owner suggestions and compensation order; persistence is
//! [`TransferStore`]. This adapter maps errors to [`ServiceError`] and publishes
//! [`DomainEvent::RepositoryTransferred`] before the best-effort audit.

use rbit_core::{Account, AccountId, RepoId, Repository, User, repository::OwnedRepository};
use rbit_git::relocate::relocate;
use rbit_repositories::transfer::{RepositoryTransfer, TransferError, TransferPort};
use rbit_storage::TransferStore;

use crate::{
    AuditService, DomainEvent, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
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
        RepositoryTransfer::suggested_owners(&TransferStore::new(&self.ctx.database), actor, &repo.repo, repo.access.can_admin())
            .await
            .map_err(service_error)
    }

    /// Moves `repo` to the account called `new_owner` and returns it as now owned.
    pub async fn transfer(
        &self,
        actor: &User,
        repo: &RepoContext,
        new_owner: &str,
        confirmation: &str,
    ) -> Result<OwnedRepository, ServiceError> {
        let bridge = TransferBridge { service: self, repo, store: TransferStore::new(&self.ctx.database) };
        RepositoryTransfer::execute(&bridge, actor, &repo.repo, repo.access.can_admin(), new_owner, confirmation)
            .await
            .map_err(service_error)
    }
}

// Borrowed request projection; no store handle exists before target lookup.
#[derive(Debug)]
struct TransferBridge<'a> {
    service: &'a TransferService,
    repo: &'a RepoContext,
    store: TransferStore<'a>,
}

impl<'a> TransferPort for TransferBridge<'a> {
    type Store = TransferStore<'a>;
    type Error = ServiceError;
    type MoveError = rbit_git::GitError;

    fn repository(&self) -> &TransferStore<'a> {
        &self.store
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

fn service_error<E: Into<ServiceError>>(error: TransferError<E>) -> ServiceError {
    match error {
        TransferError::Forbidden => ServiceError::Forbidden,
        TransferError::Invalid(message) => ServiceError::Invalid(message),
        TransferError::Failure(error) => error.into(),
    }
}
