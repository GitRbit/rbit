//! Team visibility, management authority, input rules and grant targeting.

use super::{RepositoryRef, TeamChange, TeamRepository, Unique};
use crate::organizations::{
    AccessError, Actor, MemberDirectory, OrgRole, OrganizationRef, UserAccount, find_user, invalid, require_manager,
};

const MAX_DESCRIPTION_LEN: usize = 350;

type Outcome<T, P> = Result<T, AccessError<<P as MemberDirectory>::Error>>;

/// Raw team form input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct TeamInput {
    pub name: String,
    pub description: String,
}

/// A team loaded for one viewer.
#[derive(Debug, Clone)]
pub struct OpenedTeam<T> {
    pub team: T,
    /// The viewer may manage the team (organisation owners and site administrators).
    pub can_manage: bool,
    /// The viewer is on the team.
    pub is_member: bool,
}

/// Team rules over one request's repository port.
pub struct Teams<'a, P> {
    port: &'a P,
}

impl<P> std::fmt::Debug for Teams<'_, P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Teams").finish_non_exhaustive()
    }
}

impl<'a, P: TeamRepository> Teams<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }

    /// The viewer's role in `org`, if they may see its teams at all. Site administrators
    /// count as owners; anonymous viewers and nonmembers get `NotFound`.
    pub async fn access(&self, viewer: Option<Actor<P::Id>>, org: P::Id) -> Outcome<OrgRole, P> {
        let Some(actor) = viewer else {
            return Err(AccessError::NotFound);
        };
        if actor.site_admin {
            return Ok(OrgRole::Owner);
        }
        self.port.role_of(org, actor.id).await?.ok_or(AccessError::NotFound)
    }

    /// Whether `viewer` may see `org`'s teams (members and site administrators).
    pub async fn can_view(&self, viewer: Option<Actor<P::Id>>, org: P::Id) -> Outcome<bool, P> {
        match self.access(viewer, org).await {
            Ok(_) => Ok(true),
            Err(AccessError::NotFound) => Ok(false),
            Err(other) => Err(other),
        }
    }

    /// The teams of `org`, by name.
    pub async fn list(&self, viewer: Option<Actor<P::Id>>, org: P::Id) -> Outcome<Vec<P::Summary>, P> {
        self.access(viewer, org).await?;
        Ok(self.port.list(org).await?)
    }

    /// Loads the team called `name` for `viewer`.
    pub async fn open(&self, viewer: Option<Actor<P::Id>>, org: P::Id, name: &str) -> Outcome<OpenedTeam<P::Team>, P> {
        let role = self.access(viewer, org).await?;
        let team = self.port.find_by_name(org, name).await?.ok_or(AccessError::NotFound)?;
        let is_member = match viewer {
            Some(actor) => self.port.is_member(P::team_id(&team), actor.id).await?,
            None => false,
        };
        Ok(OpenedTeam { team, can_manage: role.is_owner(), is_member })
    }

    /// Creates a team in `org`.
    pub async fn create(&self, actor: Actor<P::Id>, org: P::Id, input: &TeamInput) -> Outcome<P::Team, P> {
        require_manager(self.port, actor, org).await?;
        let (name, description) = validate::<P>(input)?;
        name_taken(self.port.create(org, &name, description.as_deref()).await?)
    }

    /// Renames the team and replaces its description; returns the new name.
    pub async fn update(&self, actor: Actor<P::Id>, org: P::Id, team: P::TeamId, input: &TeamInput) -> Outcome<P::TeamName, P> {
        require_manager(self.port, actor, org).await?;
        let (name, description) = validate::<P>(input)?;
        name_taken(self.port.update(team, &name, description.as_deref()).await?)?;
        Ok(name)
    }

    /// Deletes the team, its memberships and its repository grants.
    pub async fn delete(&self, actor: Actor<P::Id>, org: P::Id, team: P::TeamId) -> Outcome<(), P> {
        require_manager(self.port, actor, org).await?;
        Ok(self.port.delete(team).await?)
    }

    /// Members of the team, by name.
    pub async fn members(&self, team: P::TeamId) -> Outcome<Vec<P::Member>, P> {
        Ok(self.port.members(team).await?)
    }

    /// Adds the organisation member called `username`; returns the added account.
    pub async fn add_member(
        &self,
        actor: Actor<P::Id>,
        org: OrganizationRef<'_, P::Id>,
        team: P::TeamId,
        username: &str,
    ) -> Outcome<UserAccount<P::Id>, P> {
        require_manager(self.port, actor, org.id).await?;
        let user = find_user(self.port, username.trim()).await?;
        self.insert(org, team, user).await
    }

    /// Adds `username` unless they are on the team already (idempotent `PUT`). Returns
    /// the account when it was added.
    pub async fn ensure_member(
        &self,
        actor: Actor<P::Id>,
        org: OrganizationRef<'_, P::Id>,
        team: P::TeamId,
        username: &str,
    ) -> Outcome<Option<UserAccount<P::Id>>, P> {
        require_manager(self.port, actor, org.id).await?;
        let user = find_user(self.port, username.trim()).await?;
        if self.port.is_member(team, user.id).await? {
            return Ok(None);
        }
        self.add_member(actor, org, team, &user.name).await.map(Some)
    }

    /// Removes `username`: managers may remove anyone, members themselves.
    pub async fn remove_member(&self, actor: Actor<P::Id>, org: P::Id, team: P::TeamId, username: &str) -> Outcome<UserAccount<P::Id>, P> {
        let user = find_user(self.port, username.trim()).await?;
        if user.id == actor.id {
            self.access(Some(actor), org).await?;
        } else {
            require_manager(self.port, actor, org).await?;
        }
        if !self.port.remove_member(team, user.id).await? {
            return Err(invalid(format!("{} is not on this team", user.name)));
        }
        Ok(user)
    }

    /// The repositories the team was granted that `viewer` may read.
    pub async fn repositories(&self, viewer: Option<Actor<P::Id>>, team: P::TeamId) -> Outcome<Vec<P::GrantedRepository>, P> {
        let site_admin = viewer.is_some_and(|actor| actor.site_admin);
        Ok(self.port.repositories(team, viewer.map(|actor| actor.id), site_admin).await?)
    }

    /// Grants `permission` on the organisation's repository called `name` (or changes
    /// it); returns the repository's full name.
    pub async fn set_repository(
        &self,
        actor: Actor<P::Id>,
        org: OrganizationRef<'_, P::Id>,
        team: P::TeamId,
        name: &str,
        permission: P::Permission,
    ) -> Outcome<String, P> {
        require_manager(self.port, actor, org.id).await?;
        let repo = self.find_repository(org, name).await?;
        match self.port.set_repository(team, repo.id, permission).await? {
            TeamChange::Done => Ok(repo.full_name),
            // The repository was transferred away between the lookup and the grant.
            TeamChange::OutsideOrganization => Err(not_in_org(org.name, name)),
        }
    }

    /// Revokes the team's access to the repository called `name`; returns its full name.
    pub async fn remove_repository(
        &self,
        actor: Actor<P::Id>,
        org: OrganizationRef<'_, P::Id>,
        team: P::TeamId,
        name: &str,
    ) -> Outcome<String, P> {
        require_manager(self.port, actor, org.id).await?;
        let repo = self.find_repository(org, name).await?;
        if !self.port.remove_repository(team, repo.id).await? {
            return Err(invalid(format!("this team has no access to {}", repo.full_name)));
        }
        Ok(repo.full_name)
    }

    async fn insert(&self, org: OrganizationRef<'_, P::Id>, team: P::TeamId, user: UserAccount<P::Id>) -> Outcome<UserAccount<P::Id>, P> {
        match self.port.add_member(team, user.id).await? {
            Unique::Done(TeamChange::Done) => Ok(user),
            Unique::Done(TeamChange::OutsideOrganization) => {
                Err(invalid(format!("{} is not a member of {}; add them to the organization first", user.name, org.name)))
            }
            Unique::Taken => Err(invalid(format!("{} is already on this team", user.name))),
        }
    }

    /// Only the organisation's own repositories: `org/name` or a bare `name`.
    async fn find_repository(&self, org: OrganizationRef<'_, P::Id>, name: &str) -> Outcome<RepositoryRef<P::RepoId>, P> {
        let name = name.trim();
        let name = match name.split_once('/') {
            Some((owner, repo)) if owner.eq_ignore_ascii_case(org.name) => repo,
            Some(_) => return Err(not_in_org(org.name, name)),
            None => name,
        };
        if name.is_empty() {
            return Err(invalid("enter a repository name".to_owned()));
        }
        self.port.find_repository(org.name, name).await?.ok_or_else(|| not_in_org(org.name, name))
    }
}

fn not_in_org<E>(org: &str, name: &str) -> AccessError<E> {
    invalid(format!("{org} has no repository called {name}"))
}

fn name_taken<T, E>(write: Unique<T>) -> Result<T, AccessError<E>> {
    match write {
        Unique::Done(value) => Ok(value),
        Unique::Taken => Err(invalid("a team with this name already exists".to_owned())),
    }
}

fn validate<P: TeamRepository>(input: &TeamInput) -> Outcome<(P::TeamName, Option<String>), P> {
    let name: P::TeamName =
        input.name.trim().parse().map_err(|error: <P::TeamName as std::str::FromStr>::Err| invalid(error.to_string()))?;
    let description = input.description.trim();
    if description.chars().count() > MAX_DESCRIPTION_LEN {
        return Err(invalid(format!("description must be at most {MAX_DESCRIPTION_LEN} characters")));
    }
    if description.chars().any(char::is_control) {
        return Err(invalid("description must be a single line of text".to_owned()));
    }
    Ok((name, (!description.is_empty()).then(|| description.to_owned())))
}
