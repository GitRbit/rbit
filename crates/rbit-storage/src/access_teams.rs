//! Team rule adapter: the existing SQL stays authoritative for organisation-only members
//! and grants, and per-organisation name uniqueness; this only projects records.

use std::future::Future;

use rbit_access::{
    OrgRole,
    organizations::{MemberDirectory, UserAccount},
    teams::{RepositoryRef, TeamChange, TeamRepository, Unique},
};
use rbit_core::{
    Account, AccountId, RepoId,
    membership::{Permission, Team, TeamId, TeamName, TeamSummary},
    repository::OwnedRepository,
};

use crate::{
    AccountStore, Database, RepositoryStore, StorageError,
    access_organizations::find_user,
    sqlite::{orgs::OrgStore, teams::TeamStore},
};

/// Team, organisation, account and repository stores behind the `rbit-access` team port.
#[derive(Debug, Clone)]
pub struct TeamAccessStore {
    teams: TeamStore,
    orgs: OrgStore,
    accounts: AccountStore,
    repositories: RepositoryStore,
}

impl TeamAccessStore {
    pub fn new(database: &Database) -> Self {
        Self { teams: database.teams(), orgs: database.orgs(), accounts: database.accounts(), repositories: database.repositories() }
    }
}

/// A uniqueness conflict becomes `Taken`; every other failure keeps its identity.
fn unique<T>(result: Result<T, StorageError>) -> Result<Unique<T>, StorageError> {
    match result {
        Ok(value) => Ok(Unique::Done(value)),
        Err(StorageError::Conflict(_)) => Ok(Unique::Taken),
        Err(other) => Err(other),
    }
}

impl MemberDirectory for TeamAccessStore {
    type Error = StorageError;
    type Id = AccountId;

    fn find_user(&self, name: &str) -> impl Future<Output = Result<Option<UserAccount<AccountId>>, StorageError>> + Send {
        find_user(&self.accounts, name)
    }

    fn role_of(&self, org: AccountId, user: AccountId) -> impl Future<Output = Result<Option<OrgRole>, StorageError>> + Send {
        self.orgs.role_of(org, user)
    }
}

impl TeamRepository for TeamAccessStore {
    type TeamId = TeamId;
    type TeamName = TeamName;
    type Team = Team;
    type Summary = TeamSummary;
    type Member = Account;
    type RepoId = RepoId;
    type Permission = Permission;
    type GrantedRepository = (OwnedRepository, Permission);

    fn team_id(team: &Team) -> TeamId {
        team.id
    }

    async fn create(&self, org: AccountId, name: &TeamName, description: Option<&str>) -> Result<Unique<Team>, StorageError> {
        unique(self.teams.create(org, name, description).await)
    }

    fn find_by_name(&self, org: AccountId, name: &str) -> impl Future<Output = Result<Option<Team>, StorageError>> + Send {
        self.teams.find_by_name(org, name)
    }

    fn list(&self, org: AccountId) -> impl Future<Output = Result<Vec<TeamSummary>, StorageError>> + Send {
        self.teams.list(org)
    }

    async fn update(&self, team: TeamId, name: &TeamName, description: Option<&str>) -> Result<Unique<()>, StorageError> {
        unique(self.teams.update(team, name, description).await)
    }

    fn delete(&self, team: TeamId) -> impl Future<Output = Result<(), StorageError>> + Send {
        self.teams.delete(team)
    }

    fn members(&self, team: TeamId) -> impl Future<Output = Result<Vec<Account>, StorageError>> + Send {
        self.teams.members(team)
    }

    fn is_member(&self, team: TeamId, user: AccountId) -> impl Future<Output = Result<bool, StorageError>> + Send {
        self.teams.is_member(team, user)
    }

    async fn add_member(&self, team: TeamId, user: AccountId) -> Result<Unique<TeamChange>, StorageError> {
        unique(self.teams.add_member(team, user).await)
    }

    fn remove_member(&self, team: TeamId, user: AccountId) -> impl Future<Output = Result<bool, StorageError>> + Send {
        self.teams.remove_member(team, user)
    }

    fn repositories(
        &self,
        team: TeamId,
        viewer: Option<AccountId>,
        site_admin: bool,
    ) -> impl Future<Output = Result<Vec<(OwnedRepository, Permission)>, StorageError>> + Send {
        self.teams.repositories(team, viewer, site_admin)
    }

    async fn find_repository(&self, owner: &str, name: &str) -> Result<Option<RepositoryRef<RepoId>>, StorageError> {
        let repo = self.repositories.find_by_path(owner, name).await?;
        Ok(repo.map(|repo| RepositoryRef { id: repo.repo.id, full_name: repo.full_name() }))
    }

    fn set_repository(
        &self,
        team: TeamId,
        repo: RepoId,
        permission: Permission,
    ) -> impl Future<Output = Result<TeamChange, StorageError>> + Send {
        self.teams.set_repository(team, repo, permission)
    }

    fn remove_repository(&self, team: TeamId, repo: RepoId) -> impl Future<Output = Result<bool, StorageError>> + Send {
        self.teams.remove_repository(team, repo)
    }
}
