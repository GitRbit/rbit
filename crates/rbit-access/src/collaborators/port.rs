//! Persistence port for collaborator rules. Record types pass through unchanged.

use std::future::Future;

use crate::{organizations::UserAccount, teams::Unique};

/// Collaborator entries plus the account facts the rules need.
pub trait CollaboratorRepository: Sync {
    type Error: Send;
    type Id: Copy + Eq + Send + Sync;
    type Repo: Copy + Send + Sync;
    type Permission: Copy + Eq + Send + Sync;
    type Collaborator: Send;

    /// The *user* account called `name`; organisation accounts are `None`.
    fn find_user(&self, name: &str) -> impl Future<Output = Result<Option<UserAccount<Self::Id>>, Self::Error>> + Send;
    fn is_suspended(&self, user: Self::Id) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// The repository's collaborators, by name.
    fn list(&self, repo: Self::Repo) -> impl Future<Output = Result<Vec<Self::Collaborator>, Self::Error>> + Send;
    fn permission(&self, repo: Self::Repo, user: Self::Id) -> impl Future<Output = Result<Option<Self::Permission>, Self::Error>> + Send;
    /// `Taken` when the user already collaborates.
    fn add(
        &self,
        repo: Self::Repo,
        user: Self::Id,
        permission: Self::Permission,
        added_by: Self::Id,
    ) -> impl Future<Output = Result<Unique<()>, Self::Error>> + Send;
    /// Whether an entry was changed.
    fn set_permission(
        &self,
        repo: Self::Repo,
        user: Self::Id,
        permission: Self::Permission,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Whether an entry was removed.
    fn remove(&self, repo: Self::Repo, user: Self::Id) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
