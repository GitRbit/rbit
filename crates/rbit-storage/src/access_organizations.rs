//! Organisation rule adapter: the existing SQL stays authoritative for the last-owner
//! invariant, membership uniqueness and empty-before-delete; this only projects records.

use std::future::Future;

use rbit_access::{
    OrgRole,
    organizations::{
        MemberAddition, MemberDirectory, MembershipChange, NewOrganization, OrgProfileUpdate, OrganizationRepository, UserAccount,
    },
};
use rbit_core::{AccountId, AccountKind, AccountName, BasePermission, OrgMember, OrgMembership, Organization};

use crate::{AccountStore, Database, StorageError, sqlite::orgs::OrgStore};

/// Organisation and account stores behind the `rbit-access` organisation ports.
#[derive(Debug, Clone)]
pub struct OrganizationAccessStore {
    orgs: OrgStore,
    accounts: AccountStore,
}

impl OrganizationAccessStore {
    pub fn new(database: &Database) -> Self {
        Self { orgs: database.orgs(), accounts: database.accounts() }
    }
}

/// The user account called `name`; organisations are never members.
pub(crate) async fn find_user(accounts: &AccountStore, name: &str) -> Result<Option<UserAccount<AccountId>>, StorageError> {
    Ok(accounts
        .find_account_by_name(name)
        .await?
        .filter(|account| account.kind == AccountKind::User)
        .map(|account| UserAccount { id: account.id, name: account.name.to_string() }))
}

impl MemberDirectory for OrganizationAccessStore {
    type Error = StorageError;
    type Id = AccountId;

    fn find_user(&self, name: &str) -> impl Future<Output = Result<Option<UserAccount<AccountId>>, StorageError>> + Send {
        find_user(&self.accounts, name)
    }

    fn role_of(&self, org: AccountId, user: AccountId) -> impl Future<Output = Result<Option<OrgRole>, StorageError>> + Send {
        self.orgs.role_of(org, user)
    }
}

impl OrganizationRepository for OrganizationAccessStore {
    type Name = AccountName;
    type Organization = Organization;
    type Member = OrgMember;
    type Membership = OrgMembership;
    type BasePermission = BasePermission;

    fn create(&self, new: NewOrganization<AccountName, AccountId>) -> impl Future<Output = Result<Organization, StorageError>> + Send {
        self.orgs.create(new)
    }

    fn find_by_name(&self, name: &str) -> impl Future<Output = Result<Option<Organization>, StorageError>> + Send {
        self.orgs.find_by_name(name)
    }

    fn members(&self, org: AccountId) -> impl Future<Output = Result<Vec<OrgMember>, StorageError>> + Send {
        self.orgs.members(org)
    }

    fn memberships_of(&self, user: AccountId) -> impl Future<Output = Result<Vec<OrgMembership>, StorageError>> + Send {
        self.orgs.memberships_of(user)
    }

    async fn add_member(&self, org: AccountId, user: AccountId, role: OrgRole) -> Result<MemberAddition, StorageError> {
        match self.orgs.add_member(org, user, role).await {
            Ok(()) => Ok(MemberAddition::Added),
            Err(StorageError::Conflict(_)) => Ok(MemberAddition::AlreadyMember),
            Err(other) => Err(other),
        }
    }

    fn set_role(
        &self,
        org: AccountId,
        user: AccountId,
        role: OrgRole,
    ) -> impl Future<Output = Result<MembershipChange, StorageError>> + Send {
        self.orgs.set_role(org, user, role)
    }

    fn remove_member(&self, org: AccountId, user: AccountId) -> impl Future<Output = Result<MembershipChange, StorageError>> + Send {
        self.orgs.remove_member(org, user)
    }

    fn update_profile(&self, org: AccountId, update: OrgProfileUpdate) -> impl Future<Output = Result<(), StorageError>> + Send {
        self.orgs.update_profile(org, update)
    }

    fn base_permission(&self, org: AccountId) -> impl Future<Output = Result<BasePermission, StorageError>> + Send {
        self.orgs.base_permission(org)
    }

    fn set_base_permission(&self, org: AccountId, permission: BasePermission) -> impl Future<Output = Result<(), StorageError>> + Send {
        self.orgs.set_base_permission(org, permission)
    }

    fn delete_if_empty(&self, org: AccountId) -> impl Future<Output = Result<bool, StorageError>> + Send {
        self.orgs.delete_if_empty(org)
    }

    fn count_repositories(&self, org: AccountId) -> impl Future<Output = Result<i64, StorageError>> + Send {
        self.orgs.count_repositories(org)
    }
}
