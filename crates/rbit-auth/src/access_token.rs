//! Personal access token secrets.
//!
//! Format: `rbit_` + 43 base64url characters (256 random bits). The fixed prefix lets
//! secret scanners recognise leaked tokens and lets the Git endpoint tell a token from a
//! password without a database lookup.
//!
//! Tokens issued before the prefix changed start with `rit_`
//! ([`AccessTokenSecret::LEGACY_PREFIX`]) and keep working: the stored digest covers only
//! the random part, so both spellings of the same secret resolve to the same token.

use std::fmt;

use crate::secret::{self, SECRET_BYTES, TokenDigest};

/// The secret a user pastes into Git as their password. Shown once, never stored.
#[derive(Clone)]
pub struct AccessTokenSecret([u8; SECRET_BYTES]);

impl AccessTokenSecret {
    /// The prefix of every token issued now.
    pub const PREFIX: &'static str = "rbit_";
    /// The prefix of tokens issued by earlier releases, still accepted.
    pub const LEGACY_PREFIX: &'static str = "rit_";
    /// Characters of the encoded secret kept as a non-secret hint for the token list.
    const HINT_LEN: usize = 4;

    pub fn generate() -> Self {
        Self(secret::random_bytes())
    }

    /// Parses a full token string. Returns `None` unless it is exactly a rbit token, written
    /// with the current prefix or the legacy one.
    pub fn parse(value: &str) -> Option<Self> {
        Self::strip_prefix(value).and_then(secret::decode).map(Self)
    }

    /// Cheap check used to route credentials: does this look like a token at all?
    pub fn looks_like_token(value: &str) -> bool {
        Self::strip_prefix(value).is_some()
    }

    fn strip_prefix(value: &str) -> Option<&str> {
        value.strip_prefix(Self::PREFIX).or_else(|| value.strip_prefix(Self::LEGACY_PREFIX))
    }

    pub fn encode(&self) -> String {
        format!("{}{}", Self::PREFIX, secret::encode(&self.0))
    }

    pub fn digest(&self) -> TokenDigest {
        secret::digest(&self.0)
    }

    /// The last few characters, e.g. `x7Qa`, safe to display and store. They come from the
    /// random part, so a token's hint is the same whichever prefix it is written with.
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
        assert!(encoded.starts_with("rbit_"));
        assert_eq!(encoded.len(), 5 + 43);
        assert_eq!(AccessTokenSecret::parse(&encoded).unwrap().digest(), secret.digest());
        assert!(encoded.ends_with(&secret.hint()));
    }

    #[test]
    fn legacy_prefix_parses_to_the_same_secret() {
        let secret = AccessTokenSecret::generate();
        let legacy = secret.encode().replacen("rbit_", "rit_", 1);
        assert!(AccessTokenSecret::looks_like_token(&legacy));
        let parsed = AccessTokenSecret::parse(&legacy).unwrap();
        assert_eq!(parsed.digest(), secret.digest());
        assert_eq!(parsed.hint(), secret.hint());
        assert!(legacy.ends_with(&parsed.hint()));
    }

    #[test]
    fn rejects_passwords_and_session_tokens() {
        assert!(AccessTokenSecret::parse("hunter2").is_none());
        assert!(!AccessTokenSecret::looks_like_token("hunter2"));
        assert!(AccessTokenSecret::parse(&crate::SessionToken::generate().encode()).is_none());
        assert!(AccessTokenSecret::parse("rbit_tooshort").is_none());
        assert!(AccessTokenSecret::parse("rit_tooshort").is_none());
        // One prefix only: `rbit_rit_…` is not a token.
        let doubled = format!("rbit_rit_{}", &AccessTokenSecret::generate().encode()[5..]);
        assert!(AccessTokenSecret::parse(&doubled).is_none());
    }
}
