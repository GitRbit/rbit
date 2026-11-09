//! Organisations: accounts that own repositories on behalf of a group of users.
//!
//! An organisation is an [`Account`] of kind [`crate::AccountKind::Organization`] plus
//! profile fields and a member list. Every member has exactly one [`OrgRole`].

use crate::Account;

/// Role ownership lives in `rbit-access`; this path stays for existing callers.
pub use rbit_access::OrgRole;

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
