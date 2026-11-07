//! Validated identifiers that appear in URLs and on disk.
//!
//! Each type can only be constructed through [`FromStr`]/[`TryFrom`], so holding one is
//! proof that the value is safe to embed in a URL path and cannot escape a directory.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

pub use rbit_contracts::NameError;
use rbit_contracts::names::validate_slug;

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
    /// The URL name of a repository (`/{owner}/{repo}`). Unique per owner, case-insensitively.
    RepoName, kind = "repository name", max = 100, reserved = RESERVED_REPO_NAMES
);
