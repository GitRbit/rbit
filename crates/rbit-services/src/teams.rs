//! Team service adapter: maps the `rbit-access` team rules onto service errors, tracing
//! and the audit log. Visibility, management authority, input validation and grant
//! targeting live in [`rbit_access::teams`]; the storage adapter keeps organisation-only
//! members and grants atomic with each write.

use rbit_access::teams::Teams;
use rbit_core::{
    Account, Organization, User,
    membership::{Permission, Team, TeamName, TeamSummary},
    repository::OwnedRepository,
};
use rbit_storage::TeamAccessStore;

pub use rbit_access::teams::TeamInput;

use crate::{
    AuditService, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    orgs::{actor, org_ref, service_error},
};

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
    store: TeamAccessStore,
    audit: AuditService,
}

impl TeamService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { store: TeamAccessStore::new(&ctx.database), audit: AuditService::new(ctx) }
    }

    fn rules(&self) -> Teams<'_, TeamAccessStore> {
        Teams::new(&self.store)
    }

    /// Whether `viewer` may see `org`'s teams (members and site administrators).
    pub async fn can_view(&self, viewer: Option<&User>, org: &Organization) -> Result<bool, ServiceError> {
        self.rules().can_view(viewer.map(actor), org.account.id).await.map_err(service_error)
    }

    /// The teams of `org`, by name.
    pub async fn list(&self, viewer: Option<&User>, org: &Organization) -> Result<Vec<TeamSummary>, ServiceError> {
        self.rules().list(viewer.map(actor), org.account.id).await.map_err(service_error)
    }

    /// Loads the team called `name` for `viewer`.
    pub async fn open(&self, viewer: Option<&User>, org: &Organization, name: &str) -> Result<TeamContext, ServiceError> {
        let opened = self.rules().open(viewer.map(actor), org.account.id, name).await.map_err(service_error)?;
        Ok(TeamContext { org: org.clone(), team: opened.team, can_manage: opened.can_manage, is_member: opened.is_member })
    }

    /// Creates a team in `org`.
    pub async fn create(&self, actor: &User, org: &Organization, input: TeamInput) -> Result<Team, ServiceError> {
        let team = self.rules().create(self::actor(actor), org.account.id, &input).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), team = %team.name, by = %actor.name(), "team created");
        self.audit.record(AuditEntry::new(AuditAction::TeamCreated).by(actor).on(format!("{}/{}", org.name(), team.name))).await;
        Ok(team)
    }

    /// Renames the team and replaces its description; returns the new name.
    pub async fn update(&self, actor: &User, team: &TeamContext, input: TeamInput) -> Result<TeamName, ServiceError> {
        let rules = self.rules();
        let name = rules.update(self::actor(actor), team.org.account.id, team.team.id, &input).await.map_err(service_error)?;
        let detail = if name == team.team.name { "description changed".to_owned() } else { format!("renamed to {name}") };
        tracing::info!(team = %team.full_name(), by = %actor.name(), %detail, "team changed");
        self.record(AuditAction::TeamChanged, actor, team, detail).await;
        Ok(name)
    }

    /// Deletes the team, its memberships and its repository grants.
    pub async fn delete(&self, actor: &User, team: &TeamContext) -> Result<(), ServiceError> {
        self.rules().delete(self::actor(actor), team.org.account.id, team.team.id).await.map_err(service_error)?;
        tracing::info!(team = %team.full_name(), by = %actor.name(), "team deleted");
        self.audit.record(AuditEntry::new(AuditAction::TeamDeleted).by(actor).on(team.full_name())).await;
        Ok(())
    }

    /// Members of the team, by name.
    pub async fn members(&self, team: &TeamContext) -> Result<Vec<Account>, ServiceError> {
        self.rules().members(team.team.id).await.map_err(service_error)
    }

    /// Adds the organisation member called `username` to the team.
    pub async fn add_member(&self, actor: &User, team: &TeamContext, username: &str) -> Result<(), ServiceError> {
        let rules = self.rules();
        let user = rules.add_member(self::actor(actor), org_ref(&team.org), team.team.id, username).await.map_err(service_error)?;
        self.record(AuditAction::TeamChanged, actor, team, format!("added {}", user.name)).await;
        Ok(())
    }

    /// Adds the organisation member called `username` to the team unless they are on it
    /// already (the API's idempotent `PUT`). Returns whether they were added.
    pub async fn ensure_member(&self, actor: &User, team: &TeamContext, username: &str) -> Result<bool, ServiceError> {
        let rules = self.rules();
        let added = rules.ensure_member(self::actor(actor), org_ref(&team.org), team.team.id, username).await.map_err(service_error)?;
        let Some(user) = added else {
            return Ok(false);
        };
        self.record(AuditAction::TeamChanged, actor, team, format!("added {}", user.name)).await;
        Ok(true)
    }

    /// Removes `username` from the team: managers may remove anyone, members themselves.
    pub async fn remove_member(&self, actor: &User, team: &TeamContext, username: &str) -> Result<(), ServiceError> {
        let rules = self.rules();
        let user = rules.remove_member(self::actor(actor), team.org.account.id, team.team.id, username).await.map_err(service_error)?;
        self.record(AuditAction::TeamChanged, actor, team, format!("removed {}", user.name)).await;
        Ok(())
    }

    /// The repositories the team was granted that `viewer` may read, by name.
    pub async fn repositories(
        &self,
        viewer: Option<&User>,
        team: &TeamContext,
    ) -> Result<Vec<(OwnedRepository, Permission)>, ServiceError> {
        self.rules().repositories(viewer.map(actor), team.team.id).await.map_err(service_error)
    }

    /// Grants the team `permission` on the organisation's repository called `name` (or
    /// changes the permission it has).
    pub async fn set_repository(&self, actor: &User, team: &TeamContext, name: &str, permission: Permission) -> Result<(), ServiceError> {
        let rules = self.rules();
        let repo =
            rules.set_repository(self::actor(actor), org_ref(&team.org), team.team.id, name, permission).await.map_err(service_error)?;
        self.record(AuditAction::TeamChanged, actor, team, format!("{repo} ({permission})")).await;
        Ok(())
    }

    /// Revokes the team's access to the repository called `name`.
    pub async fn remove_repository(&self, actor: &User, team: &TeamContext, name: &str) -> Result<(), ServiceError> {
        let rules = self.rules();
        let repo = rules.remove_repository(self::actor(actor), org_ref(&team.org), team.team.id, name).await.map_err(service_error)?;
        self.record(AuditAction::TeamChanged, actor, team, format!("removed {repo}")).await;
        Ok(())
    }

    async fn record(&self, action: AuditAction, actor: &User, team: &TeamContext, detail: String) {
        self.audit.record(AuditEntry::new(action).by(actor).on(team.full_name()).detail(detail)).await;
    }
}
