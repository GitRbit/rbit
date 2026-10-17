//! Organisations: accounts that own repositories on behalf of a group of users.
//!
//! An organisation is an [`Account`] of kind [`crate::AccountKind::Organization`] plus
//! profile fields and a member list. Every member has exactly one [`OrgRole`].

use serde::{Deserialize, Serialize};

use crate::Account;

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

/// An organisation with its public profile.
#[derive(Debug, Clone)]
pub struct Organization {
    pub account: Account,
    pub description: Option<String>,
    /// An `http(s)://` URL, validated before it is stored.
    pub website: Option<String>,
}

impl Organization {
    pub fn name(&self) -> &crate::AccountName {
        &self.account.name
    }

    /// The display name when set, otherwise the handle.
    pub fn label(&self) -> &str {
        self.account.label()
    }
}

/// A user's membership as seen from the organisation (the members list).
#[derive(Debug, Clone)]
pub struct OrgMember {
    pub user: Account,
    pub role: OrgRole,
}

/// A user's membership as seen from the user (the dashboard, the owner picker).
#[derive(Debug, Clone)]
pub struct OrgMembership {
    pub org: Account,
    pub role: OrgRole,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_round_trip_and_order() {
        for role in OrgRole::ALL {
            assert_eq!(OrgRole::parse(role.as_str()), Some(role));
        }
        assert_eq!(OrgRole::parse("admin"), None);
        assert!(OrgRole::Owner > OrgRole::Member);
        assert!(OrgRole::Owner.is_owner() && !OrgRole::Member.is_owner());
    }
}
