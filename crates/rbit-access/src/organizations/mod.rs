//! Organisation creation policy, management authority, profile rules and membership changes.
//!
//! Rules:
//! * Who may create organisations follows the instance [`OrgCreationPolicy`]; the
//!   creator becomes the first owner.
//! * Owners and site administrators manage members, the profile, the base permission and
//!   deletion. Members are refused; nonmembers are told the organisation is not there.
//! * An organisation always keeps at least one owner, and one that still owns
//!   repositories cannot be deleted. The storage adapter settles both atomically.
//!
//! These rules authenticate nothing: actors, roles and records are trusted inputs from
//! the identity layer and the [`OrganizationRepository`] adapter.

mod application;
mod model;
mod port;
pub(crate) mod text;

pub use application::{Organizations, ViewerRole, can_create_organization};
pub(crate) use application::{find_user, invalid, require_manager};
pub use model::{
    AccessError, Actor, CreateOrganization, MembershipChange, NewOrganization, OrgCreationPolicy, OrgProfileUpdate, OrgRole,
    OrganizationRef, UpdateOrganization, UserAccount,
};
pub use port::{MemberAddition, MemberDirectory, OrganizationRepository};

#[cfg(test)]
pub(crate) mod tests;
