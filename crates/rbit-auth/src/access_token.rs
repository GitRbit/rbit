//! Personal access token secrets.
//!
//! Format: `rit_` + 43 base64url characters (256 random bits). The fixed prefix lets
//! secret scanners recognise leaked tokens and lets the Git endpoint tell a token from a
//! password without a database lookup.

use std::fmt;

use crate::secret::{self, SECRET_BYTES, TokenDigest};

/// The secret a user pastes into Git as their password. Shown once, never stored.
#[derive(Clone)]
pub struct AccessTokenSecret([u8; SECRET_BYTES]);

impl AccessTokenSecret {
    pub const PREFIX: &'static str = "rit_";
    /// Characters of the encoded secret kept as a non-secret hint for the token list.
    const HINT_LEN: usize = 4;

    pub fn generate() -> Self {
        Self(secret::random_bytes())
    }

    /// Parses a full token string. Returns `None` unless it is exactly a rbit token.
    pub fn parse(value: &str) -> Option<Self> {
        value.strip_prefix(Self::PREFIX).and_then(secret::decode).map(Self)
    }

    /// Cheap check used to route credentials: does this look like a token at all?
    pub fn looks_like_token(value: &str) -> bool {
        value.starts_with(Self::PREFIX)
    }

    pub fn encode(&self) -> String {
        format!("{}{}", Self::PREFIX, secret::encode(&self.0))
    }

    pub fn digest(&self) -> TokenDigest {
        secret::digest(&self.0)
    }

    /// The last few characters, e.g. `…x7Qa`, safe to display and store.
    pub fn hint(&self) -> String {
        let encoded = self.encode();
        encoded[encoded.len() - Self::HINT_LEN..].to_owned()
    }
}

impl fmt::Debug for AccessTokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessTokenSecret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_prefix() {
        let secret = AccessTokenSecret::generate();
        let encoded = secret.encode();
        assert!(encoded.starts_with("rit_"));
        assert_eq!(encoded.len(), 4 + 43);
        assert_eq!(AccessTokenSecret::parse(&encoded).unwrap().digest(), secret.digest());
        assert!(encoded.ends_with(&secret.hint()));
    }

    #[test]
    fn rejects_passwords_and_session_tokens() {
        assert!(AccessTokenSecret::parse("hunter2").is_none());
        assert!(AccessTokenSecret::parse(&crate::SessionToken::generate().encode()).is_none());
        assert!(AccessTokenSecret::parse("rit_tooshort").is_none());
    }
}
