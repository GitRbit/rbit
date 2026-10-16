//! Personal access tokens: long-lived credentials for Git over HTTPS and (later) the API.

use crate::{AccountId, RepoAccess, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessTokenId(pub i64);

/// What a token may do. A token never grants more than its owner has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenScope {
    /// Clone and fetch.
    #[default]
    Read,
    /// Clone, fetch and push.
    Write,
}

impl TokenScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            _ => None,
        }
    }

    /// Caps the owner's access to what this scope allows.
    pub fn limit(self, access: RepoAccess) -> RepoAccess {
        let ceiling = match self {
            Self::Read => RepoAccess::Read,
            Self::Write => RepoAccess::Write,
        };
        access.min(ceiling)
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
    }
}
