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

use super::{TokenDigest, codec as secret};
use secret::SECRET_BYTES;

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
