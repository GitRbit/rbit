//! Organisation roles, policies and the facts and inputs the organisation rules exchange.

use serde::{Deserialize, Serialize};

/// What a user may do within an organisation.
///
/// Variants are ordered, so `role >= OrgRole::Owner` reads naturally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OrgRole {
    /// Push to the organisation's repositories and create new ones.
    Member,
    /// Everything a member can do, plus administering repositories, managing members
    /// and changing or deleting the organisation.
    Owner,
}

impl OrgRole {
    /// Every role, lowest first (the order role pickers list them in).
    pub const ALL: [Self; 2] = [Self::Member, Self::Owner];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Member => "member",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "owner" => Some(Self::Owner),
            "member" => Some(Self::Member),
            _ => None,
        }
    }

    /// Capitalised name for display.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Owner => "Owner",
            Self::Member => "Member",
        }
    }

    pub const fn is_owner(self) -> bool {
        matches!(self, Self::Owner)
    }
}

/// Who may create organisations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrgCreationPolicy {
    #[default]
    AnyUser,
    AdminsOnly,
}

impl OrgCreationPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AnyUser => "any_user",
            Self::AdminsOnly => "admins_only",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "any_user" => Some(Self::AnyUser),
            "admins_only" => Some(Self::AdminsOnly),
            _ => None,
        }
    }
}

/// An authenticated actor. `site_admin` must come from the identity layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor<I> {
    pub id: I,
    pub site_admin: bool,
}

/// The organisation a rule applies to: its account ID and handle (used in messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrganizationRef<'a, I> {
    pub id: I,
    pub name: &'a str,
}

/// A user account resolved by name. Directories never return organisation accounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccount<I> {
    pub id: I,
    pub name: String,
}

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

/// Everything needed to create an organisation. Values are already validated.
#[derive(Debug)]
pub struct NewOrganization<N, I> {
    pub name: N,
    pub display_name: Option<String>,
    pub description: Option<String>,
    /// Becomes the first owner.
    pub creator: I,
}

/// New profile values. Values are already validated; `None` clears a field.
#[derive(Debug, Default)]
pub struct OrgProfileUpdate {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
}

/// Result of a membership change that must keep at least one owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipChange {
    Done,
    /// The user is not a member of the organisation.
    NotMember,
    /// Refused: the change would leave the organisation without an owner.
    LastOwner,
}

/// Rule outcomes. Messages of `Invalid` are safe to display; `Store` keeps the adapter
/// failure's identity so callers map it exactly as before.
#[derive(Debug)]
pub enum AccessError<E> {
    /// The actor may not know the resource exists.
    NotFound,
    /// The actor may see the resource but not perform this action.
    Forbidden,
    Invalid(String),
    Store(E),
}

impl<E> From<E> for AccessError<E> {
    fn from(error: E) -> Self {
        Self::Store(error)
    }
}
