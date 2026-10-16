//! Repository permissions granted to people: collaborators and organisation teams.
//!
//! The permission levels (`read`, `write`, `admin`), the organisation base permission,
//! collaborators, teams and their members and repositories live here as validated
//! domain types. Who ends up with which [`RepoAccess`] is decided by
//! `rustit_auth::policy::repo_access`; this module only describes what was granted.
//!
//! Milestone 2 workstream A (access) owns this file.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{Account, AccountId, NameError, RepoAccess, Timestamp};

/// A permission granted on a repository, to a collaborator or a team.
///
/// Variants are ordered, so the best of several grants is simply their maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Browse, clone and fetch.
    Read,
    /// Everything `Read` allows, plus pushing.
    Write,
    /// Everything `Write` allows, plus settings, collaborators, transfer and deletion.
    Admin,
}

impl Permission {
    /// Every level, lowest first (the order permission pickers list them in).
    pub const ALL: [Self; 3] = [Self::Read, Self::Write, Self::Admin];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    /// Capitalised name for display.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Write => "Write",
            Self::Admin => "Admin",
        }
    }

    /// One sentence describing what the level allows.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Read => "Can browse and clone.",
            Self::Write => "Can browse, clone and push.",
            Self::Admin => "Can push and manage settings, collaborators and transfer.",
        }
    }

    /// The repository access this permission amounts to.
    pub const fn access(self) -> RepoAccess {
        match self {
            Self::Read => RepoAccess::Read,
            Self::Write => RepoAccess::Write,
            Self::Admin => RepoAccess::Admin,
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What every member of an organisation gets on every repository of that organisation,
/// before teams and collaborators are considered. Stored as `none`/`read`/`write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub enum BasePermission {
    /// Members only see what their teams (or public visibility) give them.
    None,
    Read,
    /// The default, which matches milestone 1: members push to every repository.
    #[default]
    Write,
}

impl BasePermission {
    /// Every level, lowest first.
    pub const ALL: [Self; 3] = [Self::None, Self::Read, Self::Write];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "No permission",
            Self::Read => "Read",
            Self::Write => "Write",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::None => "Members only see public repositories and those their teams can access.",
            Self::Read => "Members can browse and clone every repository.",
            Self::Write => "Members can browse, clone and push to every repository.",
        }
    }

    /// The permission it grants, if any.
    pub const fn permission(self) -> Option<Permission> {
        match self {
            Self::None => None,
            Self::Read => Some(Permission::Read),
            Self::Write => Some(Permission::Write),
        }
    }
}

/// Stable primary key of a team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TeamId(pub i64);

/// Team names that would collide with team routes (`/{org}/teams/new`).
const RESERVED_TEAM_NAMES: &[&str] = &["new"];

/// The URL name of a team (`/{org}/teams/{team}`): letters, digits, `-`, `_` and `.`,
/// alphanumeric at both ends, unique per organisation case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TeamName(String);

impl TeamName {
    pub const MAX_LEN: usize = 50;
    const KIND: &'static str = "team name";

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), NameError> {
        let kind = Self::KIND;
        if value.is_empty() {
            return Err(NameError::Empty { kind });
        }
        if value.len() > Self::MAX_LEN {
            return Err(NameError::TooLong { kind, max: Self::MAX_LEN });
        }
        if !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
            return Err(NameError::InvalidCharacter { kind });
        }
        let bytes = value.as_bytes();
        if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
            return Err(NameError::InvalidBoundary { kind });
        }
        let lower = value.to_ascii_lowercase();
        if RESERVED_TEAM_NAMES.contains(&lower.as_str()) || lower.contains("..") {
            return Err(NameError::Reserved { kind, value: value.to_owned() });
        }
        Ok(())
    }
}

impl FromStr for TeamName {
    type Err = NameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for TeamName {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl From<TeamName> for String {
    fn from(name: TeamName) -> Self {
        name.0
    }
}

impl fmt::Display for TeamName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A team inside an organisation.
#[derive(Debug, Clone)]
pub struct Team {
    pub id: TeamId,
    pub org_id: AccountId,
    pub name: TeamName,
    pub description: Option<String>,
    pub created_at: Timestamp,
}

/// A team with its member and repository counts (the teams list).
#[derive(Debug, Clone)]
pub struct TeamSummary {
    pub team: Team,
    pub member_count: i64,
    pub repo_count: i64,
}

/// A user given a permission on one repository directly.
#[derive(Debug, Clone)]
pub struct Collaborator {
    pub user: Account,
    pub permission: Permission,
    pub added_at: Timestamp,
}

/// A team's permission on one repository, as listed on the repository's collaborators page.
#[derive(Debug, Clone)]
pub struct TeamGrant {
    pub team: Team,
    pub permission: Permission,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_round_trip_order_and_map_to_access() {
        for permission in Permission::ALL {
            assert_eq!(Permission::parse(permission.as_str()), Some(permission));
            assert_eq!(permission.to_string(), permission.as_str());
        }
        assert_eq!(Permission::parse("owner"), None);
        assert!(Permission::Read < Permission::Write && Permission::Write < Permission::Admin);
        assert_eq!(Permission::Read.access(), RepoAccess::Read);
        assert_eq!(Permission::Write.access(), RepoAccess::Write);
        assert_eq!(Permission::Admin.access(), RepoAccess::Admin);
    }

    #[test]
    fn base_permissions_round_trip_and_default_to_write() {
        for base in BasePermission::ALL {
            assert_eq!(BasePermission::parse(base.as_str()), Some(base));
        }
        assert_eq!(BasePermission::parse("admin"), None, "the base permission never grants admin");
        assert_eq!(BasePermission::default(), BasePermission::Write);
        assert_eq!(BasePermission::None.permission(), None);
        assert_eq!(BasePermission::Read.permission(), Some(Permission::Read));
        assert_eq!(BasePermission::Write.permission(), Some(Permission::Write));
    }

    #[test]
    fn team_names_are_slugs() {
        for name in ["core", "Release-Managers", "a", "web.ui", "x_1"] {
            assert!(name.parse::<TeamName>().is_ok(), "{name}");
        }
        for name in ["", "new", "NEW", "-core", "core-", "a b", "a/b", "x..y", "é", &"a".repeat(51)] {
            assert!(name.parse::<TeamName>().is_err(), "{name:?}");
        }
    }
}
