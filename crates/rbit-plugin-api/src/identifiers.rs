//! Canonical names for plugins, service contracts and granted capabilities.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// Why a plugin identifier was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdentifierError {
    #[error("identifier must be between 3 and 128 ASCII bytes")]
    Length,
    #[error("identifier must contain at least two dot-separated segments")]
    Namespace,
    #[error("identifier segments must contain lowercase ASCII letters, digits or hyphens and start and end with a letter or digit")]
    Segment,
}

fn validate(value: &str) -> Result<(), IdentifierError> {
    if !(3..=128).contains(&value.len()) {
        return Err(IdentifierError::Length);
    }
    if !value.contains('.') {
        return Err(IdentifierError::Namespace);
    }
    for segment in value.split('.') {
        let boundary = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if !segment.bytes().next().is_some_and(boundary)
            || !segment.bytes().next_back().is_some_and(boundary)
            || !segment.bytes().all(|byte| boundary(byte) || byte == b'-')
        {
            return Err(IdentifierError::Segment);
        }
    }
    Ok(())
}

macro_rules! identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Maximum identifier length, measured in ASCII bytes.
            pub const MAX_LEN: usize = 128;

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                validate(value)?;
                Ok(Self(value.to_owned()))
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                validate(&value)?;
                Ok(Self(value))
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

identifier!(PluginId, "Canonical plugin identity such as `rbit.actions`.");
identifier!(ServiceId, "Namespaced service contract identity such as `rbit.repositories`.");
identifier!(CapabilityId, "Namespaced capability identity such as `repository.read`.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_names_round_trip_without_normalization() {
        for value in ["rbit.actions", "example.git-tools.v2", "a.1", "example.a--b"] {
            let id: PluginId = value.parse().unwrap();
            assert_eq!(id.as_str(), value);
            assert_eq!(id.to_string(), value);
            let encoded = serde_json::to_string(&id).unwrap();
            assert_eq!(serde_json::from_str::<PluginId>(&encoded).unwrap(), id);
        }
    }

    #[test]
    fn identifiers_reject_ambiguous_and_path_unsafe_spellings() {
        for value in [
            "",
            "rbit",
            "Rbit.actions",
            ".rbit",
            "rbit.",
            "rbit..actions",
            "rbit.-actions",
            "rbit.actions-",
            "rbit/../actions",
            "rbit.actions\n",
            "rbit.é",
            "rbit.some_thing",
            " rbit.actions",
        ] {
            assert!(value.parse::<PluginId>().is_err(), "{value:?}");
            let encoded = serde_json::to_string(value).unwrap();
            assert!(serde_json::from_str::<PluginId>(&encoded).is_err(), "{value:?}");
            assert!(value.parse::<ServiceId>().is_err(), "{value:?}");
            assert!(value.parse::<CapabilityId>().is_err(), "{value:?}");
        }
    }

    #[test]
    fn identifier_length_is_bounded() {
        assert!(format!("a.{}", "b".repeat(126)).parse::<PluginId>().is_ok());
        assert!(format!("a.{}", "b".repeat(127)).parse::<PluginId>().is_err());
    }
}
