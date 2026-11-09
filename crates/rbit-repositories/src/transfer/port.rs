//! Persistence and request-effect ports for one transfer command.
//!
//! Persistence owns uniqueness and the owner/grant transaction. Each persistence call
//! opens the store handle it needs, so a refused command allocates none.

use std::future::Future;

use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::{Account, User};

use crate::{OwnedRepository, Repository, creation::RepositoryOwnerCatalog};

/// SQL uniqueness is authoritative even when the destination directory is absent.
#[derive(Debug)]
pub enum TransferWriteError<E> {
    Conflict,
    Failure(E),
}

/// Destination lookup, suspension, membership and the owner-change transaction.
pub trait TransferRepository: Sync {
    type Error: Send;
    /// One catalog serves one destination lookup: the site-administrator name lookup or
    /// the shared creation-owner grant.
    type Owners: RepositoryOwnerCatalog<Error = Self::Error>;
    /// The actor's organizations in the store's order, projected lazily.
    type Organizations: IntoIterator<Item = Account>;

    fn owners(&self) -> Self::Owners;
    fn is_suspended(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn organizations_of(&self, member: AccountId) -> impl Future<Output = Result<Self::Organizations, Self::Error>> + Send;
    /// Owner/timestamp update, old team grant deletion and redundant collaborator
    /// deletion in one transaction; a name clash is [`TransferWriteError::Conflict`].
    fn transfer(&self, repo: RepoId, owner: AccountId) -> impl Future<Output = Result<(), TransferWriteError<Self::Error>>> + Send;
}

/// Static request-local effects. Implementations must not add destination prechecks
/// or re-read the loaded repository: the write transaction settles concurrent moves.
pub trait TransferPort: Sync {
    type Store: TransferRepository;
    type Error: Send + From<<Self::Store as TransferRepository>::Error>;
    type MoveError: Send;

    fn repository(&self) -> &Self::Store;
    fn close_reader(&self);
    fn relocate(&self, repo: RepoId, from: AccountId, to: AccountId) -> impl Future<Output = Result<(), Self::MoveError>> + Send;
    fn move_error(&self, error: Self::MoveError) -> Self::Error;
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
