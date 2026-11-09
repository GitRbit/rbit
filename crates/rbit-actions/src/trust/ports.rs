//! The facts pull request trust is decided from, read by the composition's adapters.
use std::future::Future;

use rbit_access::RepoAccess;

/// An account as the base repository sees it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorFacts {
    /// Its access to the base repository, read now (a former writer is a reader).
    pub access: RepoAccess,
    pub suspended: bool,
}

/// One base repository's view of the accounts around a pull request. Statically
/// dispatched and borrowed; every answer is read when asked, never cached.
pub trait TrustFacts: Sync {
    /// The adapter's account id.
    type Account: Copy + PartialEq + Send + Sync;
    type Error: Send;

    /// `id` now, or `None` when the account is gone.
    fn actor(&self, id: Self::Account) -> impl Future<Output = Result<Option<ActorFacts>, Self::Error>> + Send;
    fn is_suspended(&self, id: Self::Account) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Listed in the repository's trusted users.
    fn is_trusted_user(&self, id: Self::Account) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Whether the repository's approval policy (capped by its owner's) lets a merged
    /// pull request vouch for its author; not under `all_external_contributors`.
    fn merged_pull_vouches(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// `id` authored a pull request merged into the repository.
    fn has_merged_pull(&self, id: Self::Account) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
