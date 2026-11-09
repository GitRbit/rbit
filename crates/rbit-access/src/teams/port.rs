//! Persistence port for team rules. Storage keeps race authority: team members must be
//! organisation members and grants must name the organisation's own repositories, both
//! checked in the same statement as the write; names are unique per organisation.

use std::{fmt::Display, future::Future, str::FromStr};

use crate::organizations::MemberDirectory;

/// A write guarded by a uniqueness constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unique<T> {
    Done(T),
    /// Another row already holds the unique value.
    Taken,
}

/// A change whose organisation precondition the store checks atomically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamChange {
    Done,
    /// Adding a member: the user is not a member of the team's organisation.
    /// Granting a repository: it does not belong to the team's organisation.
    OutsideOrganization,
}

/// A repository resolved by owner and name for a grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRef<R> {
    pub id: R,
    /// `owner/name`, as shown in the audit log.
    pub full_name: String,
}

/// Team records, memberships and repository grants. Record types pass through unchanged.
pub trait TeamRepository: MemberDirectory {
    type TeamId: Copy + Send + Sync;
    /// A validated team name; its parse error text is shown to the user.
    type TeamName: FromStr<Err: Display> + Send + Sync;
    type Team: Send;
    type Summary: Send;
    type Member: Send;
    type RepoId: Copy + Send + Sync;
    type Permission: Copy + Send + Sync;
    type GrantedRepository: Send;

    fn team_id(team: &Self::Team) -> Self::TeamId;
    fn create(
        &self,
        org: Self::Id,
        name: &Self::TeamName,
        description: Option<&str>,
    ) -> impl Future<Output = Result<Unique<Self::Team>, Self::Error>> + Send;
    fn find_by_name(&self, org: Self::Id, name: &str) -> impl Future<Output = Result<Option<Self::Team>, Self::Error>> + Send;
    fn list(&self, org: Self::Id) -> impl Future<Output = Result<Vec<Self::Summary>, Self::Error>> + Send;
    fn update(
        &self,
        team: Self::TeamId,
        name: &Self::TeamName,
        description: Option<&str>,
    ) -> impl Future<Output = Result<Unique<()>, Self::Error>> + Send;
    /// Deletes the team, its memberships and its repository grants.
    fn delete(&self, team: Self::TeamId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn members(&self, team: Self::TeamId) -> impl Future<Output = Result<Vec<Self::Member>, Self::Error>> + Send;
    fn is_member(&self, team: Self::TeamId, user: Self::Id) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// `Taken` when the user is already on the team.
    fn add_member(&self, team: Self::TeamId, user: Self::Id) -> impl Future<Output = Result<Unique<TeamChange>, Self::Error>> + Send;
    /// Whether a membership was removed.
    fn remove_member(&self, team: Self::TeamId, user: Self::Id) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// The team's repositories that `viewer` may read (all of them for site administrators).
    fn repositories(
        &self,
        team: Self::TeamId,
        viewer: Option<Self::Id>,
        site_admin: bool,
    ) -> impl Future<Output = Result<Vec<Self::GrantedRepository>, Self::Error>> + Send;
    fn find_repository(
        &self,
        owner: &str,
        name: &str,
    ) -> impl Future<Output = Result<Option<RepositoryRef<Self::RepoId>>, Self::Error>> + Send;
    fn set_repository(
        &self,
        team: Self::TeamId,
        repo: Self::RepoId,
        permission: Self::Permission,
    ) -> impl Future<Output = Result<TeamChange, Self::Error>> + Send;
    /// Whether a grant was revoked.
    fn remove_repository(&self, team: Self::TeamId, repo: Self::RepoId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
