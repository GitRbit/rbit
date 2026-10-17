//! Session tokens.
//!
//! The cookie holds 256 random bits encoded as base64url. The server stores only the
//! SHA-256 digest, so the session table is useless to an attacker who copies it.

use std::fmt;

use crate::secret::{self, SECRET_BYTES, TokenDigest};

/// The secret half of a session: lives only in the user's cookie.
#[derive(Clone)]
pub struct SessionToken([u8; SECRET_BYTES]);

impl SessionToken {
    /// Draws a new token from the operating system's CSPRNG.
    pub fn generate() -> Self {
        Self(secret::random_bytes())
    }

    /// Parses the cookie value. Returns `None` for anything that is not exactly a token.
    pub fn parse(encoded: &str) -> Option<Self> {
        secret::decode(encoded).map(Self)
    }

    /// The value to place in the cookie.
    pub fn encode(&self) -> String {
        secret::encode(&self.0)
    }

    pub fn digest(&self) -> TokenDigest {
        secret::digest(&self.0)
    }
}

/// Never print secrets, even in debug logs.
impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_parse_round_trip() {
        let token = SessionToken::generate();
        let parsed = SessionToken::parse(&token.encode()).expect("round trip");
        assert_eq!(parsed.digest(), token.digest());
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(SessionToken::parse("short").is_none());
        assert!(SessionToken::parse("").is_none());
    }
}
