//! rbit Actions task tokens (`docs/design/actions.md` §4.8, §11.3, §11.11).
//!
//! A [`TaskTokenSecret`] (`rbat_` + 43 base64url characters, 256 random bits) is minted when
//! a runner claims a job and travels to it once, inside the job specification. The server
//! stores only its SHA-256 digest on the task row and clears it the moment the task stops
//! or its job is cancelled, so the token dies with the task. A retried poll mints a new
//! one: the old value was never stored, so it cannot be handed out again.
//!
//! The prefix is `rbit_core::actions::TASK_TOKEN_PREFIX`: Git, LFS and the API recognise a
//! task token by it without a lookup, never try it as a password or personal access token,
//! and refuse it outright while Actions is disabled.

use std::fmt;

use rbit_core::actions::TASK_TOKEN_PREFIX;
use subtle::ConstantTimeEq;

use crate::secret::{self, SECRET_BYTES, TokenDigest};

/// The token one task acts with.
#[derive(Clone)]
pub struct TaskTokenSecret([u8; SECRET_BYTES]);

impl TaskTokenSecret {
    pub const PREFIX: &'static str = TASK_TOKEN_PREFIX;

    pub fn generate() -> Self {
        Self(secret::random_bytes())
    }

    /// Parses a full token; `None` unless it is exactly the prefix and 43 base64url
    /// characters.
    pub fn parse(value: &str) -> Option<Self> {
        value.strip_prefix(Self::PREFIX).and_then(secret::decode).map(Self)
    }

    pub fn encode(&self) -> String {
        format!("{}{}", Self::PREFIX, secret::encode(&self.0))
    }

    /// The digest stored on the task while it runs.
    pub fn digest(&self) -> TokenDigest {
        secret::digest(&self.0)
    }

    /// Whether this is the token of `digest`, in constant time.
    pub fn matches(&self, digest: &TokenDigest) -> bool {
        self.digest().ct_eq(digest).into()
    }
}

impl fmt::Debug for TaskTokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TaskTokenSecret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::actions::looks_like_task_token;

    use super::*;

    #[test]
    fn round_trips_with_the_shared_prefix() {
        let token = TaskTokenSecret::generate();
        let encoded = token.encode();
        assert!(looks_like_task_token(&encoded) && encoded.len() == 5 + 43);
        let parsed = TaskTokenSecret::parse(&encoded).unwrap();
        assert_eq!(parsed.digest(), token.digest());
        assert!(parsed.matches(&token.digest()));
        assert!(!TaskTokenSecret::generate().matches(&token.digest()));
    }

    #[test]
    fn other_secrets_are_not_task_tokens() {
        assert!(TaskTokenSecret::parse("rbat_short").is_none());
        assert!(TaskTokenSecret::parse(&crate::AccessTokenSecret::generate().encode()).is_none());
        assert!(TaskTokenSecret::parse(&crate::RunnerCredential::generate().encode()).is_none());
        assert!(!format!("{:?}", TaskTokenSecret::generate()).contains("rbat_"));
    }
}
