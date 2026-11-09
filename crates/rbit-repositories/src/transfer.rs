//! Transferring a repository to another user or organization.
//!
//! Rules:
//! * Repository administrators may move a repository to their own account or to an
//!   organization where they may create repositories (the shared
//!   [`RepositoryOwnership`] grant); site administrators may move it to any user or
//!   organization. Suspended users cannot receive repositories.
//! * The full `owner/name` must be typed to confirm. A repository of the same name at the
//!   new owner is a validation error and leaves everything as it was.
//! * The bare repository is moved first, then the owner is changed in one transaction
//!   that also drops the old organization's team grants. If that step fails a
//!   compensating move is attempted and the primary error is kept. Cancellation or a
//!   failed compensation can leave disk and metadata requiring reconciliation.
//! * Everything keyed by the repository id stays; old addresses keep resolving through
//!   [`crate::aliases`].

mod port;

use rbit_identity_models::{Account, User};

pub use port::{TransferPort, TransferRepository, TransferWriteError};

use crate::{
    OwnedRepository, Repository,
    creation::{RepositoryOwnerCatalog, RepositoryOwnerError, RepositoryOwnership},
};

/// Policy errors are displayable; adapter failures keep their original identity.
#[derive(Debug)]
pub enum TransferError<E> {
    Forbidden,
    Invalid(String),
    Failure(E),
}

/// One transfer command; administration is the loaded request's existing authority.
#[derive(Debug)]
pub struct RepositoryTransfer;

impl RepositoryTransfer {
    /// Accounts `actor` may move `current` to without typing a name (their own account
    /// and their organizations), excluding the current owner. Site administrators may
    /// also enter any other name.
    pub async fn suggested_owners<S: TransferRepository>(
        store: &S,
        actor: &User,
        current: &OwnedRepository,
        can_admin: bool,
    ) -> Result<Vec<Account>, TransferError<S::Error>> {
        if !can_admin {
            return Err(TransferError::Forbidden);
        }
        let organizations = store.organizations_of(actor.id()).await.map_err(TransferError::Failure)?;
        let owner = current.repo.owner_id;
        Ok(std::iter::once(actor.account.clone()).chain(organizations).filter(|account| account.id != owner).collect())
    }

    /// Moves `current` to the account called `new_owner` and returns it as now owned.
    pub async fn execute<P: TransferPort>(
        port: &P,
        actor: &User,
        current: &OwnedRepository,
        can_admin: bool,
        new_owner: &str,
        confirmation: &str,
    ) -> Result<OwnedRepository, TransferError<P::Error>> {
        if !can_admin {
            return Err(TransferError::Forbidden);
        }
        let full_name = current.full_name();
        let new_owner = new_owner.trim();
        if new_owner.is_empty() {
            return Err(TransferError::Invalid("choose the new owner".to_owned()));
        }
        if confirmation.trim() != full_name {
            return Err(TransferError::Invalid(format!("type \"{full_name}\" to confirm")));
        }
        let target = Self::target(port, actor, new_owner).await?;
        let previous = &current.repo;
        if target.id == previous.owner_id {
            return Err(TransferError::Invalid(format!("{full_name} already belongs to {}", target.name)));
        }
        port.close_reader();
        port.relocate(previous.id, previous.owner_id, target.id).await.map_err(|error| TransferError::Failure(port.move_error(error)))?;
        if let Err(error) = port.repository().transfer(previous.id, target.id).await {
            if let Err(restore) = port.relocate(previous.id, target.id, previous.owner_id).await {
                port.restore_failed(&restore, &full_name);
            }
            return Err(match error {
                TransferWriteError::Conflict => {
                    TransferError::Invalid(format!("{}/{} already exists; rename one of them first", target.name, previous.name))
                }
                TransferWriteError::Failure(error) => TransferError::Failure(error.into()),
            });
        }
        let moved = OwnedRepository { repo: Repository { owner_id: target.id, ..previous.clone() }, owner: target };
        port.transferred(actor, previous, &moved, full_name).await;
        Ok(moved)
    }

    async fn target<P: TransferPort>(port: &P, actor: &User, name: &str) -> Result<Account, TransferError<P::Error>> {
        let store = port.repository();
        let owners = store.owners();
        let target = if actor.is_admin {
            owners
                .owner_named(name)
                .await
                .map_err(|error| TransferError::Failure(error.into()))?
                .ok_or_else(|| TransferError::Invalid(format!("there is no user or organization called {name}")))?
        } else {
            RepositoryOwnership::resolve(&owners, actor, name).await.map_err(|error| match error {
                RepositoryOwnerError::Invalid(_) => TransferError::Invalid(format!("you cannot transfer repositories to {name}")),
                RepositoryOwnerError::Port(error) => TransferError::Failure(error.into()),
            })?
        };
        drop(owners);
        if !target.is_organization() && store.is_suspended(target.id).await.map_err(|error| TransferError::Failure(error.into()))? {
            return Err(TransferError::Invalid(format!("{} is suspended", target.name)));
        }
        Ok(target)
    }
}
