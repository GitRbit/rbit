//! Route-safe account namespace, including the existing reserved top-level routes.

use std::{fmt, str::FromStr};

use rbit_contracts::{NameError, names::validate_slug};
use serde::{Deserialize, Serialize};

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

/// The URL name of a user or organisation (`/{account}`). Case-insensitively unique.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AccountName(String);

impl AccountName {
    pub const MAX_LEN: usize = 39;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AccountName {
    type Err = NameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_slug("username", value, Self::MAX_LEN, RESERVED_ACCOUNT_NAMES)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for AccountName {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_slug("username", &value, Self::MAX_LEN, RESERVED_ACCOUNT_NAMES)?;
        Ok(Self(value))
    }
}

impl From<AccountName> for String {
    fn from(name: AccountName) -> Self {
        name.0
    }
}

impl AsRef<str> for AccountName {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
