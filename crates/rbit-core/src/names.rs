//! Validated identifiers that appear in URLs and on disk.
//!
//! Each type can only be constructed through [`FromStr`]/[`TryFrom`], so holding one is
//! proof that the value is safe to embed in a URL path and cannot escape a directory.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

pub use rbit_contracts::NameError;
use rbit_contracts::names::validate_slug;

/// Top-level paths that would collide with application routes if used as an account name.
const RESERVED_ACCOUNT_NAMES: &[&str] = &[
    "admin",
    "api",
    "assets",
    "email",
    "explore",
    "favicon.ico",
    "healthz",
    "issues",
    "login",
    "logout",
    "manifest.webmanifest",
    "metrics",
    "new",
    "notifications",
    "offline",
    "org",
    "organizations",
    "orgs",
    "password",
    "pulls",
    "readyz",
    "robots.txt",
    "settings",
    "signup",
    "sw.js",
    "themes",
    "user",
    "users",
];

/// Second-level paths reserved under `/{owner}/`.
const RESERVED_REPO_NAMES: &[&str] = &["followers", "members", "people", "repositories", "settings", "stars", "teams"];

macro_rules! validated_name {
    ($(#[$doc:meta])* $name:ident, kind = $kind:literal, max = $max:expr, reserved = $reserved:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub const MAX_LEN: usize = $max;

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = NameError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                validate_slug($kind, value, $max, $reserved)?;
                Ok(Self(value.to_owned()))
            }
        }

        impl TryFrom<String> for $name {
            type Error = NameError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                validate_slug($kind, &value, $max, $reserved)?;
                Ok(Self(value))
            }
        }

        impl From<$name> for String {
            fn from(name: $name) -> Self {
                name.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

validated_name!(
    /// The URL name of a user or organisation (`/{account}`). Case-insensitively unique.
    AccountName, kind = "username", max = 39, reserved = RESERVED_ACCOUNT_NAMES
);

validated_name!(
    /// The URL name of a repository (`/{owner}/{repo}`). Unique per owner, case-insensitively.
    RepoName, kind = "repository name", max = 100, reserved = RESERVED_REPO_NAMES
);

/// A syntactically plausible email address. Deliverability is verified separately by
/// sending a confirmation link; this type only rules out obvious garbage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Email(String);

impl Email {
    pub const MAX_LEN: usize = 254;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Email {
    type Err = NameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        let plausible = value.len() <= Self::MAX_LEN
            && !value.chars().any(char::is_whitespace)
            && value.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
            });
        if plausible { Ok(Self(value.to_owned())) } else { Err(NameError::InvalidEmail) }
    }
}

impl TryFrom<String> for Email {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Email> for String {
    fn from(email: Email) -> Self {
        email.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_names() {
        for name in ["alice", "a", "rust-lang", "my_repo.rs", "Repo2"] {
            assert!(name.parse::<RepoName>().is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn rejects_traversal_and_git_suffixes() {
        for name in ["", "..", "../etc", "a/b", ".hidden", "trailing-", "repo.git", "x..y", "has space", "é"] {
            assert!(name.parse::<RepoName>().is_err(), "{name:?} should be rejected");
        }
    }

    #[test]
    fn reserved_account_names_are_case_insensitive() {
        assert!(matches!("Settings".parse::<AccountName>(), Err(NameError::Reserved { .. })));
    }

    #[test]
    fn dashboards_are_reserved_account_names() {
        for name in ["issues", "Pulls", "notifications"] {
            assert!(matches!(name.parse::<AccountName>(), Err(NameError::Reserved { .. })), "{name}");
        }
    }

    #[test]
    fn organisation_pages_are_reserved_repository_names() {
        for name in ["members", "People", "settings", "teams"] {
            assert!(matches!(name.parse::<RepoName>(), Err(NameError::Reserved { .. })), "{name}");
        }
    }

    #[test]
    fn email_requires_domain_with_dot() {
        assert!("dev@example.com".parse::<Email>().is_ok());
        assert!("dev@localhost".parse::<Email>().is_err());
        assert!("@example.com".parse::<Email>().is_err());
    }
}
