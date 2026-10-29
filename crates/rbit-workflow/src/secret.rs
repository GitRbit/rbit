//! A secret value inside a [`crate::spec`] or [`crate::protocol`] message: zeroed when it is
//! dropped and never printed.
//!
//! Secrets, the task token, registration tokens and runner credentials all travel as
//! [`Secret`]. Its `Debug` is `Secret(***)` whatever the value and it has no `Display`, so
//! printing an enclosing message with `{:?}` cannot leak it; [`Secret::expose`] is the only
//! way to read it. Serialising writes the value itself, because it has to cross the wire.
//!
//! Deserialising goes through an owned `String` that becomes the secret, but a JSON parser
//! may briefly hold a copy in its scratch buffer (for strings with escapes) that this type
//! cannot reach. Callers that read secrets from a request body drop the body promptly.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

/// A string that is zeroed on drop and redacted in `Debug`.
#[derive(Clone, Default)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// The value. Call sites are the places a secret is used, so they stay easy to audit.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Equality without an early exit on the first differing byte; only the length can be
/// told apart. It exists for comparing messages in tests and caches, not for verifying
/// credentials, which the server does by digest.
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        let (left, right) = (self.0.as_bytes(), other.0.as_bytes());
        left.len() == right.len() && left.iter().zip(right).fold(0u8, |difference, (a, b)| difference | (a ^ b)) == 0
    }
}

impl Eq for Secret {}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.expose())
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_redacted_and_compared_by_value() {
        let secret = Secret::new("hunter2-value");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert_eq!(format!("{:?}", Some(vec![secret.clone()])), "Some([Secret(***)])");
        assert_eq!(secret.expose(), "hunter2-value");
        assert_eq!((secret.len(), secret.is_empty(), Secret::default().is_empty()), (13, false, true));
        assert_eq!(secret, Secret::from("hunter2-value".to_owned()));
        assert_ne!(secret, Secret::new("hunter2-valuf"));
        assert_ne!(secret, Secret::new("hunter2"));
    }

    #[test]
    fn secrets_serialise_as_their_value() {
        let secret = Secret::new("a\"b");
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, r#""a\"b""#);
        assert_eq!(serde_json::from_str::<Secret>(&json).unwrap(), secret);
        assert!(serde_json::from_str::<Secret>("1").is_err());
    }
}
