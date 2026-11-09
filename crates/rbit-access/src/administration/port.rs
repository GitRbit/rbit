//! Persistence port for site administration. Storage keeps race authority: the "never
//! remove the last active administrator" rule is part of each moderation statement.

use std::future::Future;

use super::{ListFilter, Listing};

/// Outcome of a guarded moderation statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moderation {
    Applied,
    /// No such user.
    NotFound,
    /// Refused: the target is the last active administrator.
    LastAdmin,
}

/// Instance-wide accounts and repositories. Record types pass through unchanged.
pub trait AdministrationRepository: Sync {
    type Error: Send;
    type Id: Copy + PartialEq + Send + Sync;
    type RepoId: Copy + Send + Sync;
    type User: Send;
    type Organization: Send;
    type Repository: Send;

    fn user_id(user: &Self::User) -> Self::Id;
    fn user_name(user: &Self::User) -> &str;
    /// `owner/name`.
    fn repository_name(repo: &Self::Repository) -> String;

    fn list_users(&self, filter: ListFilter<'_>) -> impl Future<Output = Result<Listing<Self::User>, Self::Error>> + Send;
    fn find_user(&self, id: Self::Id) -> impl Future<Output = Result<Option<Self::User>, Self::Error>> + Send;
    fn set_admin(&self, id: Self::Id, is_admin: bool) -> impl Future<Output = Result<Moderation, Self::Error>> + Send;
    fn set_suspended(&self, id: Self::Id, suspended: bool) -> impl Future<Output = Result<Moderation, Self::Error>> + Send;
    /// Deletes the user and everything they own in the database.
    fn delete_user(&self, id: Self::Id) -> impl Future<Output = Result<Moderation, Self::Error>> + Send;
    /// The repositories `owner` owns, with their names (for disk removal and events).
    fn repositories_owned_by(&self, owner: Self::Id) -> impl Future<Output = Result<Vec<(Self::RepoId, String)>, Self::Error>> + Send;
    /// Names of the organisations whose only owner is `user`.
    fn organizations_solely_owned_by(&self, user: Self::Id) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;
    fn list_organizations(&self, filter: ListFilter<'_>) -> impl Future<Output = Result<Listing<Self::Organization>, Self::Error>> + Send;
    fn list_repositories(&self, filter: ListFilter<'_>) -> impl Future<Output = Result<Listing<Self::Repository>, Self::Error>> + Send;
    fn find_repository(&self, id: Self::RepoId) -> impl Future<Output = Result<Option<Self::Repository>, Self::Error>> + Send;
    /// Whether the repository's metadata was deleted.
    fn delete_repository(&self, id: Self::RepoId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
