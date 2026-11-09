//! Organisation creation, profile, membership, base-permission and deletion rules.

use super::{
    AccessError, Actor, CreateOrganization, MemberAddition, MemberDirectory, MembershipChange, NewOrganization, OrgCreationPolicy,
    OrgProfileUpdate, OrgRole, OrganizationRef, OrganizationRepository, UpdateOrganization, UserAccount,
    text::{MAX_DESCRIPTION_LEN, MAX_DISPLAY_NAME_LEN, optional_text, website},
};

type Outcome<T, P> = Result<T, AccessError<<P as MemberDirectory>::Error>>;

/// A viewer's standing in one organisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerRole {
    /// The viewer's membership, if any.
    pub role: Option<OrgRole>,
    /// The viewer may manage members, the profile and deletion (owners and site admins).
    pub can_manage: bool,
}

/// Whether an actor may create organisations under the instance policy.
pub fn can_create_organization(policy: OrgCreationPolicy, site_admin: bool) -> bool {
    match policy {
        OrgCreationPolicy::AnyUser => true,
        OrgCreationPolicy::AdminsOnly => site_admin,
    }
}

/// Organisation rules over one request's repository port.
pub struct Organizations<'a, P> {
    port: &'a P,
}

impl<P> std::fmt::Debug for Organizations<'_, P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Organizations").finish_non_exhaustive()
    }
}

impl<'a, P: OrganizationRepository> Organizations<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }

    /// Creates an organisation with `actor` as its first owner.
    pub async fn create(&self, policy: OrgCreationPolicy, actor: Actor<P::Id>, input: &CreateOrganization) -> Outcome<P::Organization, P> {
        if !can_create_organization(policy, actor.site_admin) {
            return Err(AccessError::Forbidden);
        }
        let name: P::Name = input
            .name
            .trim()
            .parse()
            .map_err(|e: <P::Name as std::str::FromStr>::Err| invalid(e.to_string().replace("username", "organization name")))?;
        let display_name = optional_text(&input.display_name, "display name", MAX_DISPLAY_NAME_LEN).map_err(AccessError::Invalid)?;
        let description = optional_text(&input.description, "description", MAX_DESCRIPTION_LEN).map_err(AccessError::Invalid)?;
        Ok(self.port.create(NewOrganization { name, display_name, description, creator: actor.id }).await?)
    }

    /// The organisation called `name`; unknown names (and users) are `NotFound`.
    pub async fn find(&self, name: &str) -> Outcome<P::Organization, P> {
        self.port.find_by_name(name).await?.ok_or(AccessError::NotFound)
    }

    /// The viewer's role in `org`, and whether they may manage it.
    pub async fn viewer(&self, org: P::Id, viewer: Option<Actor<P::Id>>) -> Outcome<ViewerRole, P> {
        let role = match viewer {
            Some(actor) => self.port.role_of(org, actor.id).await?,
            None => None,
        };
        let can_manage = role.is_some_and(OrgRole::is_owner) || viewer.is_some_and(|actor| actor.site_admin);
        Ok(ViewerRole { role, can_manage })
    }

    pub async fn members(&self, org: P::Id) -> Outcome<Vec<P::Member>, P> {
        Ok(self.port.members(org).await?)
    }

    pub async fn memberships(&self, user: P::Id) -> Outcome<Vec<P::Membership>, P> {
        Ok(self.port.memberships_of(user).await?)
    }

    /// Adds the user called `username` with `role`; returns the added account.
    pub async fn add_member(
        &self,
        actor: Actor<P::Id>,
        org: OrganizationRef<'_, P::Id>,
        username: &str,
        role: OrgRole,
    ) -> Outcome<UserAccount<P::Id>, P> {
        self.require_manager(actor, org.id).await?;
        let user = find_user(self.port, username.trim()).await?;
        match self.port.add_member(org.id, user.id, role).await? {
            MemberAddition::Added => Ok(user),
            MemberAddition::AlreadyMember => Err(invalid(format!("{} is already a member", user.name))),
        }
    }

    /// Gives the member called `username` a new role. The last owner cannot be demoted.
    pub async fn change_role(
        &self,
        actor: Actor<P::Id>,
        org: OrganizationRef<'_, P::Id>,
        username: &str,
        role: OrgRole,
    ) -> Outcome<UserAccount<P::Id>, P> {
        self.require_manager(actor, org.id).await?;
        let user = find_user(self.port, username.trim()).await?;
        explain(self.port.set_role(org.id, user.id, role).await?, org.name, &user.name)?;
        Ok(user)
    }

    /// Removes the member called `username`. The last owner cannot be removed.
    pub async fn remove_member(
        &self,
        actor: Actor<P::Id>,
        org: OrganizationRef<'_, P::Id>,
        username: &str,
    ) -> Outcome<UserAccount<P::Id>, P> {
        self.require_manager(actor, org.id).await?;
        let user = find_user(self.port, username.trim()).await?;
        explain(self.port.remove_member(org.id, user.id).await?, org.name, &user.name)?;
        Ok(user)
    }

    /// `actor` leaves `org`. The last owner must hand ownership over first.
    pub async fn leave(&self, actor: Actor<P::Id>, org: OrganizationRef<'_, P::Id>) -> Outcome<(), P> {
        match self.port.remove_member(org.id, actor.id).await? {
            MembershipChange::Done => Ok(()),
            MembershipChange::NotMember => Err(AccessError::NotFound),
            MembershipChange::LastOwner => {
                Err(invalid(format!("you are the last owner of {}; make another member an owner before leaving", org.name)))
            }
        }
    }

    /// Replaces the profile fields. Empty values clear a field.
    pub async fn update_profile(&self, actor: Actor<P::Id>, org: P::Id, input: &UpdateOrganization) -> Outcome<(), P> {
        self.require_manager(actor, org).await?;
        let update = OrgProfileUpdate {
            display_name: optional_text(&input.display_name, "display name", MAX_DISPLAY_NAME_LEN).map_err(AccessError::Invalid)?,
            description: optional_text(&input.description, "description", MAX_DESCRIPTION_LEN).map_err(AccessError::Invalid)?,
            website: website(&input.website).map_err(AccessError::Invalid)?,
        };
        Ok(self.port.update_profile(org, update).await?)
    }

    pub async fn base_permission(&self, org: P::Id) -> Outcome<P::BasePermission, P> {
        Ok(self.port.base_permission(org).await?)
    }

    /// Changes the base permission. Owners and site administrators only.
    pub async fn set_base_permission(&self, actor: Actor<P::Id>, org: P::Id, permission: P::BasePermission) -> Outcome<(), P> {
        self.require_manager(actor, org).await?;
        Ok(self.port.set_base_permission(org, permission).await?)
    }

    /// Deletes `org`. `confirmation` must repeat its name, and it must own no repositories.
    pub async fn delete(&self, actor: Actor<P::Id>, org: OrganizationRef<'_, P::Id>, confirmation: &str) -> Outcome<(), P> {
        self.require_manager(actor, org.id).await?;
        if !confirmation.trim().eq_ignore_ascii_case(org.name) {
            return Err(invalid(format!("type {} to confirm", org.name)));
        }
        if !self.port.delete_if_empty(org.id).await? {
            let count = self.port.count_repositories(org.id).await?;
            let noun = if count == 1 { "repository" } else { "repositories" };
            return Err(invalid(format!("{} still owns {count} {noun}; delete them or transfer them to another owner first", org.name)));
        }
        Ok(())
    }

    /// Owners and site administrators manage an organisation; members are refused and
    /// everyone else is told it does not exist.
    pub async fn require_manager(&self, actor: Actor<P::Id>, org: P::Id) -> Outcome<(), P> {
        require_manager(self.port, actor, org).await
    }
}

/// Shared with the team rules: site administrators count as owners.
pub(crate) async fn require_manager<D: MemberDirectory>(port: &D, actor: Actor<D::Id>, org: D::Id) -> Result<(), AccessError<D::Error>> {
    if actor.site_admin {
        return Ok(());
    }
    match port.role_of(org, actor.id).await? {
        Some(OrgRole::Owner) => Ok(()),
        Some(OrgRole::Member) => Err(AccessError::Forbidden),
        None => Err(AccessError::NotFound),
    }
}

/// The user account called `username` (already trimmed); organisations cannot be members.
pub(crate) async fn find_user<D: MemberDirectory>(port: &D, username: &str) -> Result<UserAccount<D::Id>, AccessError<D::Error>> {
    if username.is_empty() {
        return Err(invalid("enter a username".to_owned()));
    }
    port.find_user(username).await?.ok_or_else(|| invalid(format!("there is no user called {username}")))
}

pub(crate) fn invalid<E>(message: String) -> AccessError<E> {
    AccessError::Invalid(message)
}

/// Turns a refused membership change into a user-facing error.
fn explain<E>(change: MembershipChange, org: &str, member: &str) -> Result<(), AccessError<E>> {
    match change {
        MembershipChange::Done => Ok(()),
        MembershipChange::NotMember => Err(invalid(format!("{member} is not a member of {org}"))),
        MembershipChange::LastOwner => Err(invalid(format!("{org} must keep at least one owner"))),
    }
}
