//! Organisations: creation, profile, membership and roles.
//!
//! Rules:
//! * Who may create organisations follows the instance setting
//!   ([`OrgCreationPolicy`]); the creator becomes the first owner.
//! * Owners (and site administrators) manage members, the profile and deletion.
//! * An organisation always keeps at least one owner: the last owner can be neither
//!   removed, demoted nor leave. The store enforces this atomically.
//! * An organisation that still owns repositories cannot be deleted.
//! * The base permission (what every member gets on every repository, `write` by default)
//!   is changed by owners. Leaving the organisation removes a member from its teams and
//!   drops their collaborator entries on its repositories (enforced by the store).

use rustit_core::{
    AccountKind, AccountName, OrgCreationPolicy, OrgMember, OrgMembership, OrgRole, Organization, User, membership::BasePermission,
};
use rustit_storage::{
    StorageError,
    sqlite::orgs::{MembershipChange, NewOrganization, OrgProfileUpdate},
};

use crate::{
    InstanceService, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

const MAX_DISPLAY_NAME_LEN: usize = 100;
const MAX_DESCRIPTION_LEN: usize = 350;
const MAX_WEBSITE_LEN: usize = 255;

/// Raw "new organisation" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct CreateOrganization {
    pub name: String,
    pub display_name: String,
    pub description: String,
}

/// Raw profile input, exactly as submitted. Empty fields are cleared.
#[derive(Debug, Clone, Default)]
pub struct UpdateOrganization {
    pub display_name: String,
    pub description: String,
    pub website: String,
}

/// An organisation as seen by one viewer.
#[derive(Debug, Clone)]
pub struct OrgContext {
    pub org: Organization,
    /// The viewer's membership, if any.
    pub role: Option<OrgRole>,
    /// The viewer may manage members, the profile and deletion (owners and site admins).
    pub can_manage: bool,
}

impl OrgContext {
    pub fn is_member(&self) -> bool {
        self.role.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct OrgService {
    ctx: ServiceContext,
    instance: InstanceService,
    audit: AuditService,
}

impl OrgService {
    pub(crate) fn new(ctx: ServiceContext, instance: InstanceService) -> Self {
        Self { audit: AuditService::new(ctx.clone()), ctx, instance }
    }

    /// Whether `actor` may create organisations under the current instance settings.
    pub fn can_create(&self, actor: &User) -> bool {
        match self.instance.current().org_creation {
            OrgCreationPolicy::AnyUser => true,
            OrgCreationPolicy::AdminsOnly => actor.is_admin,
        }
    }

    /// Creates an organisation with `actor` as its owner.
    pub async fn create(&self, actor: &User, input: CreateOrganization) -> Result<Organization, ServiceError> {
        if !self.can_create(actor) {
            return Err(ServiceError::Forbidden);
        }
        let name: AccountName = input
            .name
            .trim()
            .parse()
            .map_err(|e: rustit_core::NameError| ServiceError::Invalid(e.to_string().replace("username", "organization name")))?;
        let display_name = optional_text(&input.display_name, "display name", MAX_DISPLAY_NAME_LEN)?;
        let description = optional_text(&input.description, "description", MAX_DESCRIPTION_LEN)?;

        let org = self.ctx.database.orgs().create(NewOrganization { name, display_name, description, creator: actor.id() }).await?;
        tracing::info!(org = %org.name(), owner = %actor.name(), "organization created");
        Ok(org)
    }

    /// Loads the organisation called `name` for `viewer`. Users and unknown names are
    /// [`ServiceError::NotFound`].
    pub async fn open(&self, viewer: Option<&User>, name: &str) -> Result<OrgContext, ServiceError> {
        let org = self.ctx.database.orgs().find_by_name(name).await?.ok_or(ServiceError::NotFound)?;
        let role = match viewer {
            Some(user) => self.ctx.database.orgs().role_of(org.account.id, user.id()).await?,
            None => None,
        };
        let can_manage = role.is_some_and(OrgRole::is_owner) || viewer.is_some_and(|user| user.is_admin);
        Ok(OrgContext { org, role, can_manage })
    }

    /// Members of `org`, owners first.
    pub async fn members(&self, org: &Organization) -> Result<Vec<OrgMember>, ServiceError> {
        Ok(self.ctx.database.orgs().members(org.account.id).await?)
    }

    /// The organisations `user` belongs to, by name.
    pub async fn memberships(&self, user: &User) -> Result<Vec<OrgMembership>, ServiceError> {
        Ok(self.ctx.database.orgs().memberships_of(user.id()).await?)
    }

    /// Adds the user called `username` to `org` with `role`.
    pub async fn add_member(&self, actor: &User, org: &Organization, username: &str, role: OrgRole) -> Result<(), ServiceError> {
        self.require_manager(actor, org).await?;
        let username = username.trim();
        let user = self.find_user(username).await?;
        match self.ctx.database.orgs().add_member(org.account.id, user.id, role).await {
            Ok(()) => {
                tracing::info!(org = %org.name(), member = %user.name, role = role.as_str(), by = %actor.name(), "member added");
                Ok(())
            }
            Err(StorageError::Conflict(_)) => Err(ServiceError::Invalid(format!("{} is already a member", user.name))),
            Err(other) => Err(other.into()),
        }
    }

    /// Gives the member called `username` a new role. The last owner cannot be demoted.
    pub async fn change_role(&self, actor: &User, org: &Organization, username: &str, role: OrgRole) -> Result<(), ServiceError> {
        self.require_manager(actor, org).await?;
        let user = self.find_user(username.trim()).await?;
        let change = self.ctx.database.orgs().set_role(org.account.id, user.id, role).await?;
        explain(change, org, &user.name)?;
        tracing::info!(org = %org.name(), member = %user.name, role = role.as_str(), by = %actor.name(), "member role changed");
        Ok(())
    }

    /// Removes the member called `username`. The last owner cannot be removed.
    pub async fn remove_member(&self, actor: &User, org: &Organization, username: &str) -> Result<(), ServiceError> {
        self.require_manager(actor, org).await?;
        let user = self.find_user(username.trim()).await?;
        let change = self.ctx.database.orgs().remove_member(org.account.id, user.id).await?;
        explain(change, org, &user.name)?;
        tracing::info!(org = %org.name(), member = %user.name, by = %actor.name(), "member removed");
        Ok(())
    }

    /// `actor` leaves `org`. The last owner must hand ownership over first.
    pub async fn leave(&self, actor: &User, org: &Organization) -> Result<(), ServiceError> {
        match self.ctx.database.orgs().remove_member(org.account.id, actor.id()).await? {
            MembershipChange::Done => {
                tracing::info!(org = %org.name(), member = %actor.name(), "member left");
                Ok(())
            }
            MembershipChange::NotMember => Err(ServiceError::NotFound),
            MembershipChange::LastOwner => {
                Err(ServiceError::Invalid(format!("you are the last owner of {}; make another member an owner before leaving", org.name())))
            }
        }
    }

    /// Replaces the profile fields. Empty values clear a field.
    pub async fn update_profile(&self, actor: &User, org: &Organization, input: UpdateOrganization) -> Result<(), ServiceError> {
        self.require_manager(actor, org).await?;
        let update = OrgProfileUpdate {
            display_name: optional_text(&input.display_name, "display name", MAX_DISPLAY_NAME_LEN)?,
            description: optional_text(&input.description, "description", MAX_DESCRIPTION_LEN)?,
            website: website(&input.website)?,
        };
        self.ctx.database.orgs().update_profile(org.account.id, update).await?;
        tracing::info!(org = %org.name(), by = %actor.name(), "organization profile updated");
        Ok(())
    }

    /// What every member gets on every repository of `org` (before teams and collaborators).
    pub async fn base_permission(&self, org: &Organization) -> Result<BasePermission, ServiceError> {
        Ok(self.ctx.database.orgs().base_permission(org.account.id).await?)
    }

    /// Changes the base permission. Owners and site administrators only.
    pub async fn set_base_permission(&self, actor: &User, org: &Organization, permission: BasePermission) -> Result<(), ServiceError> {
        self.require_manager(actor, org).await?;
        self.ctx.database.orgs().set_base_permission(org.account.id, permission).await?;
        tracing::info!(org = %org.name(), permission = permission.as_str(), by = %actor.name(), "organization base permission changed");
        let entry = AuditEntry::new(AuditAction::OrgPermissionChanged).by(actor).on(org.name().as_str());
        self.audit.record(entry.detail(format!("members: {}", permission.as_str()))).await;
        Ok(())
    }

    /// Deletes `org`. `confirmation` must repeat its name, and it must own no repositories.
    pub async fn delete(&self, actor: &User, org: &Organization, confirmation: &str) -> Result<(), ServiceError> {
        self.require_manager(actor, org).await?;
        if !confirmation.trim().eq_ignore_ascii_case(org.name().as_str()) {
            return Err(ServiceError::Invalid(format!("type {} to confirm", org.name())));
        }
        let store = self.ctx.database.orgs();
        if !store.delete_if_empty(org.account.id).await? {
            let count = store.count_repositories(org.account.id).await?;
            let noun = if count == 1 { "repository" } else { "repositories" };
            return Err(ServiceError::Invalid(format!(
                "{} still owns {count} {noun}; delete them or transfer them to another owner first",
                org.name()
            )));
        }
        tracing::info!(org = %org.name(), by = %actor.name(), "organization deleted");
        Ok(())
    }

    /// Owners and site administrators manage an organisation; everyone else is refused.
    async fn require_manager(&self, actor: &User, org: &Organization) -> Result<(), ServiceError> {
        if actor.is_admin {
            return Ok(());
        }
        match self.ctx.database.orgs().role_of(org.account.id, actor.id()).await? {
            Some(OrgRole::Owner) => Ok(()),
            Some(OrgRole::Member) => Err(ServiceError::Forbidden),
            None => Err(ServiceError::NotFound),
        }
    }

    /// The user account called `username`; organisations cannot be members.
    async fn find_user(&self, username: &str) -> Result<rustit_core::Account, ServiceError> {
        if username.is_empty() {
            return Err(ServiceError::Invalid("enter a username".to_owned()));
        }
        match self.ctx.database.accounts().find_account_by_name(username).await? {
            Some(account) if account.kind == AccountKind::User => Ok(account),
            _ => Err(ServiceError::Invalid(format!("there is no user called {username}"))),
        }
    }
}

/// Turns a refused membership change into a user-facing error.
fn explain(change: MembershipChange, org: &Organization, member: &AccountName) -> Result<(), ServiceError> {
    match change {
        MembershipChange::Done => Ok(()),
        MembershipChange::NotMember => Err(ServiceError::Invalid(format!("{member} is not a member of {}", org.name()))),
        MembershipChange::LastOwner => Err(ServiceError::Invalid(format!("{} must keep at least one owner", org.name()))),
    }
}

/// Trims `value`; empty becomes `None`, too long is an error naming `field`.
fn optional_text(value: &str, field: &str, max: usize) -> Result<Option<String>, ServiceError> {
    let value = value.trim();
    if value.chars().count() > max {
        return Err(ServiceError::Invalid(format!("{field} must be at most {max} characters")));
    }
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

/// An optional `http(s)://` URL. Other schemes (`javascript:`, `data:`) are refused, so
/// the value is always safe to render as a link.
fn website(value: &str) -> Result<Option<String>, ServiceError> {
    let Some(url) = optional_text(value, "website", MAX_WEBSITE_LEN)? else {
        return Ok(None);
    };
    let lower = url.to_ascii_lowercase();
    let host = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
    let plausible = host.is_some_and(|rest| {
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        !host.is_empty() && !host.starts_with('.')
    }) && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if plausible {
        Ok(Some(url))
    } else {
        Err(ServiceError::Invalid("website must be a link starting with https:// or http://".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websites_must_be_http_links() {
        assert_eq!(website("").unwrap(), None);
        assert_eq!(website("  https://acme.example/about ").unwrap().as_deref(), Some("https://acme.example/about"));
        assert!(website("HTTP://acme.example").is_ok());
        for bad in ["acme.example", "javascript:alert(1)", "https://", "https:///path", "https://a b.example", "ftp://acme.example"] {
            assert!(website(bad).is_err(), "{bad:?}");
        }
        assert!(website(&format!("https://{}.example", "a".repeat(250))).is_err());
    }

    #[test]
    fn optional_text_trims_and_limits() {
        assert_eq!(optional_text("   ", "x", 3).unwrap(), None);
        assert_eq!(optional_text(" é é ", "x", 3).unwrap().as_deref(), Some("é é"));
        assert!(optional_text("abcd", "x", 3).is_err());
    }
}
