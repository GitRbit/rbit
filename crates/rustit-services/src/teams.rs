//! Organisation teams: members, and the permission each team has on repositories.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Rules:
//! * Teams, their members and their repositories are visible to members of the
//!   organisation and to site administrators; everyone else gets
//!   [`ServiceError::NotFound`].
//! * Owners (and site administrators) create, rename and delete teams, and manage their
//!   members and repositories. Members may leave a team themselves.
//! * Team members must be members of the organisation (enforced atomically by the store;
//!   leaving the organisation removes them from its teams).
//! * A team is only granted repositories of its own organisation.

use rustit_core::{
    Account, AccountKind, OrgRole, Organization, User,
    membership::{Permission, Team, TeamName, TeamSummary},
    repository::OwnedRepository,
};
use rustit_storage::{
    StorageError,
    sqlite::teams::{TeamChange, TeamStore},
};

use crate::{
    AuditService, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
};

const MAX_DESCRIPTION_LEN: usize = 350;

/// Raw team form input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct TeamInput {
    pub name: String,
    pub description: String,
}

/// A team as seen by one viewer.
#[derive(Debug, Clone)]
pub struct TeamContext {
    pub org: Organization,
    pub team: Team,
    /// The viewer may manage the team (organisation owners and site administrators).
    pub can_manage: bool,
    /// The viewer is on the team.
    pub is_member: bool,
}

impl TeamContext {
    /// `org/team`, as shown in headings and the audit log.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.org.name(), self.team.name)
    }
}

#[derive(Debug, Clone)]
pub struct TeamService {
    ctx: ServiceContext,
    audit: AuditService,
}

impl TeamService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { audit: AuditService::new(ctx.clone()), ctx }
    }

    fn store(&self) -> TeamStore {
        self.ctx.database.teams()
    }

    /// The viewer's role in `org`, if they may see its teams at all. Site administrators
    /// count as owners.
    async fn access(&self, viewer: Option<&User>, org: &Organization) -> Result<OrgRole, ServiceError> {
        let Some(user) = viewer else {
            return Err(ServiceError::NotFound);
        };
        if user.is_admin {
            return Ok(OrgRole::Owner);
        }
        self.ctx.database.orgs().role_of(org.account.id, user.id()).await?.ok_or(ServiceError::NotFound)
    }

    /// `actor` may manage `org`'s teams: owners and site administrators. Members get
    /// [`ServiceError::Forbidden`], everyone else [`ServiceError::NotFound`].
    async fn require_manager(&self, actor: &User, org: &Organization) -> Result<(), ServiceError> {
        match self.access(Some(actor), org).await? {
            OrgRole::Owner => Ok(()),
            OrgRole::Member => Err(ServiceError::Forbidden),
        }
    }

    /// Whether `viewer` may see `org`'s teams (members and site administrators).
    pub async fn can_view(&self, viewer: Option<&User>, org: &Organization) -> Result<bool, ServiceError> {
        match self.access(viewer, org).await {
            Ok(_) => Ok(true),
            Err(ServiceError::NotFound) => Ok(false),
            Err(other) => Err(other),
        }
    }

    /// The teams of `org`, by name.
    pub async fn list(&self, viewer: Option<&User>, org: &Organization) -> Result<Vec<TeamSummary>, ServiceError> {
        self.access(viewer, org).await?;
        Ok(self.store().list(org.account.id).await?)
    }

    /// Loads the team called `name` for `viewer`.
    pub async fn open(&self, viewer: Option<&User>, org: &Organization, name: &str) -> Result<TeamContext, ServiceError> {
        let role = self.access(viewer, org).await?;
        let team = self.store().find_by_name(org.account.id, name).await?.ok_or(ServiceError::NotFound)?;
        let is_member = match viewer {
            Some(user) => self.store().is_member(team.id, user.id()).await?,
            None => false,
        };
        Ok(TeamContext { org: org.clone(), team, can_manage: role.is_owner(), is_member })
    }

    /// Creates a team in `org`.
    pub async fn create(&self, actor: &User, org: &Organization, input: TeamInput) -> Result<Team, ServiceError> {
        self.require_manager(actor, org).await?;
        let (name, description) = validate(&input)?;
        let team = self.store().create(org.account.id, &name, description.as_deref()).await.map_err(name_taken)?;
        tracing::info!(org = %org.name(), team = %team.name, by = %actor.name(), "team created");
        self.audit.record(AuditEntry::new(AuditAction::TeamCreated).by(actor).on(format!("{}/{}", org.name(), team.name))).await;
        Ok(team)
    }

    /// Renames the team and replaces its description; returns the new name.
    pub async fn update(&self, actor: &User, team: &TeamContext, input: TeamInput) -> Result<TeamName, ServiceError> {
        self.require_manager(actor, &team.org).await?;
        let (name, description) = validate(&input)?;
        self.store().update(team.team.id, &name, description.as_deref()).await.map_err(name_taken)?;
        let detail = if name == team.team.name { "description changed".to_owned() } else { format!("renamed to {name}") };
        tracing::info!(team = %team.full_name(), by = %actor.name(), %detail, "team changed");
        self.record(AuditAction::TeamChanged, actor, team, detail).await;
        Ok(name)
    }

    /// Deletes the team, its memberships and its repository grants.
    pub async fn delete(&self, actor: &User, team: &TeamContext) -> Result<(), ServiceError> {
        self.require_manager(actor, &team.org).await?;
        self.store().delete(team.team.id).await?;
        tracing::info!(team = %team.full_name(), by = %actor.name(), "team deleted");
        self.audit.record(AuditEntry::new(AuditAction::TeamDeleted).by(actor).on(team.full_name())).await;
        Ok(())
    }

    /// Members of the team, by name.
    pub async fn members(&self, team: &TeamContext) -> Result<Vec<Account>, ServiceError> {
        Ok(self.store().members(team.team.id).await?)
    }

    /// Adds the organisation member called `username` to the team.
    pub async fn add_member(&self, actor: &User, team: &TeamContext, username: &str) -> Result<(), ServiceError> {
        self.require_manager(actor, &team.org).await?;
        let user = self.find_user(username).await?;
        match self.store().add_member(team.team.id, user.id).await {
            Ok(TeamChange::Done) => {}
            Ok(TeamChange::OutsideOrganization) => {
                return Err(ServiceError::Invalid(format!(
                    "{} is not a member of {}; add them to the organization first",
                    user.name,
                    team.org.name()
                )));
            }
            Err(StorageError::Conflict(_)) => return Err(ServiceError::Invalid(format!("{} is already on this team", user.name))),
            Err(other) => return Err(other.into()),
        }
        self.record(AuditAction::TeamChanged, actor, team, format!("added {}", user.name)).await;
        Ok(())
    }

    /// Removes `username` from the team: managers may remove anyone, members themselves.
    pub async fn remove_member(&self, actor: &User, team: &TeamContext, username: &str) -> Result<(), ServiceError> {
        let user = self.find_user(username).await?;
        if user.id == actor.id() {
            self.access(Some(actor), &team.org).await?;
        } else {
            self.require_manager(actor, &team.org).await?;
        }
        if !self.store().remove_member(team.team.id, user.id).await? {
            return Err(ServiceError::Invalid(format!("{} is not on this team", user.name)));
        }
        self.record(AuditAction::TeamChanged, actor, team, format!("removed {}", user.name)).await;
        Ok(())
    }

    /// The repositories the team was granted that `viewer` may read, by name.
    pub async fn repositories(
        &self,
        viewer: Option<&User>,
        team: &TeamContext,
    ) -> Result<Vec<(OwnedRepository, Permission)>, ServiceError> {
        let is_admin = viewer.is_some_and(|user| user.is_admin);
        Ok(self.store().repositories(team.team.id, viewer.map(User::id), is_admin).await?)
    }

    /// Grants the team `permission` on the organisation's repository called `name` (or
    /// changes the permission it has).
    pub async fn set_repository(&self, actor: &User, team: &TeamContext, name: &str, permission: Permission) -> Result<(), ServiceError> {
        self.require_manager(actor, &team.org).await?;
        let repo = self.find_repository(team, name).await?;
        match self.store().set_repository(team.team.id, repo.repo.id, permission).await? {
            TeamChange::Done => {}
            // The repository was transferred away between the lookup and the grant.
            TeamChange::OutsideOrganization => return Err(not_in_org(team, name)),
        }
        self.record(AuditAction::TeamChanged, actor, team, format!("{} ({permission})", repo.full_name())).await;
        Ok(())
    }

    /// Revokes the team's access to the repository called `name`.
    pub async fn remove_repository(&self, actor: &User, team: &TeamContext, name: &str) -> Result<(), ServiceError> {
        self.require_manager(actor, &team.org).await?;
        let repo = self.find_repository(team, name).await?;
        if !self.store().remove_repository(team.team.id, repo.repo.id).await? {
            return Err(ServiceError::Invalid(format!("this team has no access to {}", repo.full_name())));
        }
        self.record(AuditAction::TeamChanged, actor, team, format!("removed {}", repo.full_name())).await;
        Ok(())
    }

    async fn find_repository(&self, team: &TeamContext, name: &str) -> Result<OwnedRepository, ServiceError> {
        let name = name.trim();
        let name = name.rsplit_once('/').map_or(name, |(_, repo)| repo);
        if name.is_empty() {
            return Err(ServiceError::Invalid("enter a repository name".to_owned()));
        }
        self.ctx.database.repositories().find_by_path(team.org.name().as_str(), name).await?.ok_or_else(|| not_in_org(team, name))
    }

    /// The user account called `username`.
    async fn find_user(&self, username: &str) -> Result<Account, ServiceError> {
        let username = username.trim();
        if username.is_empty() {
            return Err(ServiceError::Invalid("enter a username".to_owned()));
        }
        match self.ctx.database.accounts().find_account_by_name(username).await? {
            Some(account) if account.kind == AccountKind::User => Ok(account),
            _ => Err(ServiceError::Invalid(format!("there is no user called {username}"))),
        }
    }

    async fn record(&self, action: AuditAction, actor: &User, team: &TeamContext, detail: String) {
        self.audit.record(AuditEntry::new(action).by(actor).on(team.full_name()).detail(detail)).await;
    }
}

fn not_in_org(team: &TeamContext, name: &str) -> ServiceError {
    ServiceError::Invalid(format!("{} has no repository called {name}", team.org.name()))
}

fn name_taken(error: StorageError) -> ServiceError {
    match error {
        StorageError::Conflict(_) => ServiceError::Invalid("a team with this name already exists".to_owned()),
        other => other.into(),
    }
}

fn validate(input: &TeamInput) -> Result<(TeamName, Option<String>), ServiceError> {
    let name: TeamName = input.name.trim().parse()?;
    let description = input.description.trim();
    if description.chars().count() > MAX_DESCRIPTION_LEN {
        return Err(ServiceError::Invalid(format!("description must be at most {MAX_DESCRIPTION_LEN} characters")));
    }
    if description.chars().any(char::is_control) {
        return Err(ServiceError::Invalid("description must be a single line of text".to_owned()));
    }
    Ok((name, (!description.is_empty()).then(|| description.to_owned())))
}
