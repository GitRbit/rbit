//! Personal access tokens: long-lived credentials for Git over HTTPS and the REST API.

use crate::{AccountId, RepoAccess, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessTokenId(pub i64);

/// What a token may do. A token never grants more than its owner has.
///
/// Scopes are ordered: each one includes everything the previous one allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TokenScope {
    /// Clone and fetch; API reads (`GET`) only.
    #[default]
    Read,
    /// Also push, and API changes that need write access (creating repositories, starring).
    Write,
    /// Also repository settings, webhooks and deletion.
    Admin,
}

impl TokenScope {
    /// Every scope, in increasing order of power (the order the tokens page lists them).
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

    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Write => "Write",
            Self::Admin => "Admin",
        }
    }

    /// Caps the owner's access to what this scope allows.
    pub fn limit(self, access: RepoAccess) -> RepoAccess {
        let ceiling = match self {
            Self::Read => RepoAccess::Read,
            Self::Write => RepoAccess::Write,
            Self::Admin => RepoAccess::Admin,
        };
        access.min(ceiling)
    }

    /// Whether this scope includes `required`.
    pub fn allows(self, required: Self) -> bool {
        self >= required
    }
}

/// Token metadata. The secret itself is shown once at creation and never stored.
#[derive(Debug, Clone)]
pub struct AccessToken {
    pub id: AccessTokenId,
    pub account_id: AccountId,
    pub name: String,
    /// The last characters of the secret, so users can tell tokens apart.
    pub hint: String,
    pub scope: TokenScope,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
    pub expires_at: Option<Timestamp>,
}

impl AccessToken {
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|at| at <= Timestamp::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_caps_access() {
        assert_eq!(TokenScope::Read.limit(RepoAccess::Admin), RepoAccess::Read);
        assert_eq!(TokenScope::Write.limit(RepoAccess::Admin), RepoAccess::Write);
        assert_eq!(TokenScope::Write.limit(RepoAccess::Read), RepoAccess::Read);
        assert_eq!(TokenScope::Write.limit(RepoAccess::None), RepoAccess::None);
        assert_eq!(TokenScope::Admin.limit(RepoAccess::Admin), RepoAccess::Admin);
        assert_eq!(TokenScope::Admin.limit(RepoAccess::Write), RepoAccess::Write, "never more than the owner has");
        assert_eq!(TokenScope::Admin.limit(RepoAccess::None), RepoAccess::None);
    }

    #[test]
    fn scopes_round_trip_and_include_weaker_ones() {
        for scope in TokenScope::ALL {
            assert_eq!(TokenScope::parse(scope.as_str()), Some(scope));
            assert!(scope.allows(TokenScope::Read));
            assert!(scope.allows(scope));
        }
        assert_eq!(TokenScope::parse("ADMIN"), None);
        assert!(!TokenScope::Read.allows(TokenScope::Write));
        assert!(!TokenScope::Write.allows(TokenScope::Admin));
        assert!(TokenScope::Admin.allows(TokenScope::Write));
    }
}
