//! Transfer authority and compensating disk/metadata ordering.
//!
//! Persistence owns uniqueness and the owner/grant transaction. Disk movement and
//! metadata are not atomic: cancellation or a failed compensating move can require
//! reconciliation. The port must retain the primary metadata error after rollback.

use std::future::Future;

use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::{Account, User};

use crate::{OwnedRepository, Repository};

/// Policy errors are displayable; adapter failures keep their original identity.
#[derive(Debug)]
pub enum TransferError<E> {
    Forbidden,
    Invalid(String),
    Failure(E),
}

/// SQL uniqueness is authoritative even when the destination directory is absent.
#[derive(Debug)]
pub enum TransferWriteError<E> {
    Conflict,
    Failure(E),
}

/// Static request-local effects. Implementations must not add destination prechecks
/// or re-read the loaded repository: the write transaction settles concurrent moves.
pub trait TransferPort: Sync {
    type Error: Send;
    type MoveError: Send;

    fn find_owner(&self, name: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send;
    fn authorized_owner(&self, actor: &User, name: &str) -> impl Future<Output = Result<Account, TransferError<Self::Error>>> + Send;
    fn is_suspended(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn close_reader(&self);
    fn relocate(&self, repo: RepoId, from: AccountId, to: AccountId) -> impl Future<Output = Result<(), Self::MoveError>> + Send;
    fn move_error(&self, error: Self::MoveError) -> Self::Error;
    fn transfer(&self, repo: RepoId, owner: AccountId) -> impl Future<Output = Result<(), TransferWriteError<Self::Error>>> + Send;
    fn restore_failed(&self, error: &Self::MoveError, old_name: &str);
    /// Publish the event before awaiting the best-effort audit, preserving that gap.
    fn transferred(
        &self,
        actor: &User,
        previous: &Repository,
        moved: &OwnedRepository,
        old_name: String,
    ) -> impl Future<Output = ()> + Send;
}

/// One transfer command; administration is the loaded request's existing authority.
#[derive(Debug)]
pub struct RepositoryTransfer;

impl RepositoryTransfer {
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
        if let Err(error) = port.transfer(previous.id, target.id).await {
            if let Err(restore) = port.relocate(previous.id, target.id, previous.owner_id).await {
                port.restore_failed(&restore, &full_name);
            }
            return Err(match error {
                TransferWriteError::Conflict => {
                    TransferError::Invalid(format!("{}/{} already exists; rename one of them first", target.name, previous.name))
                }
                TransferWriteError::Failure(error) => TransferError::Failure(error),
            });
        }
        let moved = OwnedRepository { repo: Repository { owner_id: target.id, ..previous.clone() }, owner: target };
        port.transferred(actor, previous, &moved, full_name).await;
        Ok(moved)
    }

    async fn target<P: TransferPort>(port: &P, actor: &User, name: &str) -> Result<Account, TransferError<P::Error>> {
        let target = if actor.is_admin {
            port.find_owner(name)
                .await
                .map_err(TransferError::Failure)?
                .ok_or_else(|| TransferError::Invalid(format!("there is no user or organization called {name}")))?
        } else {
            port.authorized_owner(actor, name).await.map_err(|error| match error {
                TransferError::Invalid(_) => TransferError::Invalid(format!("you cannot transfer repositories to {name}")),
                other => other,
            })?
        };
        if !target.is_organization() && port.is_suspended(target.id).await.map_err(TransferError::Failure)? {
            return Err(TransferError::Invalid(format!("{} is suspended", target.name)));
        }
        Ok(target)
    }
}
