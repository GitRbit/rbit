//! Organisation service adapter: maps the `rbit-access` organisation rules onto service
//! errors, tracing and the audit log. Creation policy, management authority, profile
//! validation, membership refusals and deletion rules live in
//! [`rbit_access::organizations`]; the storage adapter keeps the last-owner, membership
//! and empty-before-delete invariants atomic.

use rbit_access::organizations::{AccessError, Actor, OrganizationRef, Organizations};
use rbit_core::{AccountId, BasePermission, OrgMember, OrgMembership, OrgRole, Organization, User};
use rbit_storage::{OrganizationAccessStore, StorageError};

pub use rbit_access::organizations::{CreateOrganization, UpdateOrganization};

use crate::{
    InstanceService, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

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
    store: OrganizationAccessStore,
    instance: InstanceService,
    audit: AuditService,
}

/// The identity layer's facts about an authenticated user, for the access rules.
pub(crate) fn actor(user: &User) -> Actor<AccountId> {
    Actor { id: user.id(), site_admin: user.is_admin }
}

pub(crate) fn org_ref(org: &Organization) -> OrganizationRef<'_, AccountId> {
    OrganizationRef { id: org.account.id, name: org.name().as_str() }
}

/// Rule outcomes keep their meaning; storage failures map exactly as before.
pub(crate) fn service_error(error: AccessError<StorageError>) -> ServiceError {
    match error {
        AccessError::NotFound => ServiceError::NotFound,
        AccessError::Forbidden => ServiceError::Forbidden,
        AccessError::Invalid(message) => ServiceError::Invalid(message),
        AccessError::Store(error) => error.into(),
    }
}

impl OrgService {
    pub(crate) fn new(ctx: ServiceContext, instance: InstanceService) -> Self {
        Self { store: OrganizationAccessStore::new(&ctx.database), audit: AuditService::new(ctx), instance }
    }

    fn rules(&self) -> Organizations<'_, OrganizationAccessStore> {
        Organizations::new(&self.store)
    }

    /// Whether `actor` may create organisations under the current instance settings.
    pub fn can_create(&self, actor: &User) -> bool {
        rbit_access::organizations::can_create_organization(self.instance.current().org_creation, actor.is_admin)
    }

    /// Creates an organisation with `actor` as its owner.
    pub async fn create(&self, actor: &User, input: CreateOrganization) -> Result<Organization, ServiceError> {
        let policy = self.instance.current().org_creation;
        let org = self.rules().create(policy, self::actor(actor), &input).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), owner = %actor.name(), "organization created");
        Ok(org)
    }

    /// Loads the organisation called `name` for `viewer`. Users and unknown names are
    /// [`ServiceError::NotFound`].
    pub async fn open(&self, viewer: Option<&User>, name: &str) -> Result<OrgContext, ServiceError> {
        let org = self.rules().find(name).await.map_err(service_error)?;
        let standing = self.rules().viewer(org.account.id, viewer.map(actor)).await.map_err(service_error)?;
        Ok(OrgContext { org, role: standing.role, can_manage: standing.can_manage })
    }

    /// Members of `org`, owners first.
    pub async fn members(&self, org: &Organization) -> Result<Vec<OrgMember>, ServiceError> {
        self.rules().members(org.account.id).await.map_err(service_error)
    }

    /// The organisations `user` belongs to, by name.
    pub async fn memberships(&self, user: &User) -> Result<Vec<OrgMembership>, ServiceError> {
        self.rules().memberships(user.id()).await.map_err(service_error)
    }

    /// Adds the user called `username` to `org` with `role`.
    pub async fn add_member(&self, actor: &User, org: &Organization, username: &str, role: OrgRole) -> Result<(), ServiceError> {
        let user = self.rules().add_member(self::actor(actor), org_ref(org), username, role).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), member = %user.name, role = role.as_str(), by = %actor.name(), "member added");
        Ok(())
    }

    /// Gives the member called `username` a new role. The last owner cannot be demoted.
    pub async fn change_role(&self, actor: &User, org: &Organization, username: &str, role: OrgRole) -> Result<(), ServiceError> {
        let user = self.rules().change_role(self::actor(actor), org_ref(org), username, role).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), member = %user.name, role = role.as_str(), by = %actor.name(), "member role changed");
        Ok(())
    }

    /// Removes the member called `username`. The last owner cannot be removed.
    pub async fn remove_member(&self, actor: &User, org: &Organization, username: &str) -> Result<(), ServiceError> {
        let user = self.rules().remove_member(self::actor(actor), org_ref(org), username).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), member = %user.name, by = %actor.name(), "member removed");
        Ok(())
    }

    /// `actor` leaves `org`. The last owner must hand ownership over first.
    pub async fn leave(&self, actor: &User, org: &Organization) -> Result<(), ServiceError> {
        self.rules().leave(self::actor(actor), org_ref(org)).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), member = %actor.name(), "member left");
        Ok(())
    }

    /// Replaces the profile fields. Empty values clear a field.
    pub async fn update_profile(&self, actor: &User, org: &Organization, input: UpdateOrganization) -> Result<(), ServiceError> {
        self.rules().update_profile(self::actor(actor), org.account.id, &input).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), by = %actor.name(), "organization profile updated");
        Ok(())
    }

    /// What every member gets on every repository of `org` (before teams and collaborators).
    pub async fn base_permission(&self, org: &Organization) -> Result<BasePermission, ServiceError> {
        self.rules().base_permission(org.account.id).await.map_err(service_error)
    }

    /// Changes the base permission. Owners and site administrators only.
    pub async fn set_base_permission(&self, actor: &User, org: &Organization, permission: BasePermission) -> Result<(), ServiceError> {
        self.rules().set_base_permission(self::actor(actor), org.account.id, permission).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), permission = permission.as_str(), by = %actor.name(), "organization base permission changed");
        let entry = AuditEntry::new(AuditAction::OrgPermissionChanged).by(actor).on(org.name().as_str());
        self.audit.record(entry.detail(format!("members: {}", permission.as_str()))).await;
        Ok(())
    }

    /// Deletes `org`. `confirmation` must repeat its name, and it must own no repositories.
    pub async fn delete(&self, actor: &User, org: &Organization, confirmation: &str) -> Result<(), ServiceError> {
        self.rules().delete(self::actor(actor), org_ref(org), confirmation).await.map_err(service_error)?;
        tracing::info!(org = %org.name(), by = %actor.name(), "organization deleted");
        Ok(())
    }
}
