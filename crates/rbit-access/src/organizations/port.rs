//! Persistence ports for organisation rules. Storage keeps race authority: the last-owner
//! invariant, membership uniqueness and empty-before-delete are settled atomically there.

use std::{fmt::Display, future::Future, str::FromStr};

use super::{MembershipChange, NewOrganization, OrgProfileUpdate, OrgRole, UserAccount};

/// Account lookups shared by the organisation and team rules.
pub trait MemberDirectory: Sync {
    type Error: Send;
    /// Account identifier; compared by value, never by name.
    type Id: Copy + Eq + Send + Sync;

    /// The *user* account called `name` (exact lookup rules belong to the adapter).
    /// Organisation accounts are `None`: they cannot be members.
    fn find_user(&self, name: &str) -> impl Future<Output = Result<Option<UserAccount<Self::Id>>, Self::Error>> + Send;
    /// `user`'s current role in `org`, if they are a member.
    fn role_of(&self, org: Self::Id, user: Self::Id) -> impl Future<Output = Result<Option<OrgRole>, Self::Error>> + Send;
}

/// Outcome of adding a member: membership uniqueness is the store's to settle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberAddition {
    Added,
    AlreadyMember,
}

/// Organisation records and membership writes. Record types pass through unchanged.
pub trait OrganizationRepository: MemberDirectory {
    /// A validated organisation handle; its parse error text names "username".
    type Name: FromStr<Err: Display> + Send;
    type Organization: Send;
    type Member: Send;
    type Membership: Send;
    /// What every member gets on every repository; opaque to these rules.
    type BasePermission: Copy + Send;

    fn create(&self, new: NewOrganization<Self::Name, Self::Id>) -> impl Future<Output = Result<Self::Organization, Self::Error>> + Send;
    fn find_by_name(&self, name: &str) -> impl Future<Output = Result<Option<Self::Organization>, Self::Error>> + Send;
    fn members(&self, org: Self::Id) -> impl Future<Output = Result<Vec<Self::Member>, Self::Error>> + Send;
    fn memberships_of(&self, user: Self::Id) -> impl Future<Output = Result<Vec<Self::Membership>, Self::Error>> + Send;
    fn add_member(&self, org: Self::Id, user: Self::Id, role: OrgRole) -> impl Future<Output = Result<MemberAddition, Self::Error>> + Send;
    fn set_role(&self, org: Self::Id, user: Self::Id, role: OrgRole) -> impl Future<Output = Result<MembershipChange, Self::Error>> + Send;
    fn remove_member(&self, org: Self::Id, user: Self::Id) -> impl Future<Output = Result<MembershipChange, Self::Error>> + Send;
    fn update_profile(&self, org: Self::Id, update: OrgProfileUpdate) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn base_permission(&self, org: Self::Id) -> impl Future<Output = Result<Self::BasePermission, Self::Error>> + Send;
    fn set_base_permission(&self, org: Self::Id, permission: Self::BasePermission) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Deletes the organisation only when it owns no repositories, atomically.
    fn delete_if_empty(&self, org: Self::Id) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn count_repositories(&self, org: Self::Id) -> impl Future<Output = Result<i64, Self::Error>> + Send;
}
