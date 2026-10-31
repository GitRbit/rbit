//! Short-lived browser form proofs for host-authorized plugin POST routes.
//!
//! A proof is bound to one host boot, session, actor, repository, route and exact
//! registration version. It grants no authorization and is not an idempotency key.

use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{SessionToken, secret};

pub const PLUGIN_CSRF_MAX_AGE_SECONDS: u64 = 30 * 60;
pub const PLUGIN_CSRF_TOKEN_BYTES: usize = 79;
const CLOCK_LEEWAY_SECONDS: u64 = 60;
const MAX_ACTION_BYTES: usize = 512;
const PREFIX: &str = "pf1.";
const PAYLOAD_BYTES: usize = 24;
const DECODED_BYTES: usize = PAYLOAD_BYTES + 32;
type FormMac = Hmac<Sha256>;

/// Facts supplied by the browser host after session and route authorization.
#[derive(Debug, Clone, Copy)]
pub struct PluginCsrfBinding<'a> {
    pub action: &'a str,
    pub actor_id: i64,
    pub repository_id: Option<i64>,
    pub generation: u64,
    pub version: u64,
}

impl PluginCsrfBinding<'_> {
    fn valid(&self) -> bool {
        self.actor_id > 0
            && self.repository_id.is_none_or(|id| id > 0)
            && self.generation != 0
            && self.version != 0
            && !self.action.is_empty()
            && self.action.len() <= MAX_ACTION_BYTES
            && self.action.starts_with('/')
            && !self.action.starts_with("//")
            && self.action.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~'))
            && self.action[1..].split('/').all(|part| !part.is_empty() && !matches!(part, "." | ".."))
    }
}

/// A random host-boot signing key. Cloning a host shares this identity; recreating
/// the host invalidates its previously issued forms without storing browser tokens.
#[derive(Clone)]
pub struct PluginCsrfKey([u8; 32]);

impl fmt::Debug for PluginCsrfKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PluginCsrfKey(<redacted>)")
    }
}

impl PluginCsrfKey {
    pub fn generate() -> Self {
        Self(secret::random_bytes())
    }

    /// Fresh randomness prevents repeated forms from reflecting a stable secret
    /// into compressed HTML. `now` is the host's Unix time in seconds.
    pub fn issue(&self, session: &SessionToken, binding: &PluginCsrfBinding<'_>, now: u64) -> Option<String> {
        if !binding.valid() {
            return None;
        }
        let mut bytes = [0; DECODED_BYTES];
        bytes[..8].copy_from_slice(&now.to_be_bytes());
        bytes[8..PAYLOAD_BYTES].copy_from_slice(&secret::random_bytes()[..16]);
        let tag = self.mac(session, binding, &bytes[..PAYLOAD_BYTES]).finalize().into_bytes();
        bytes[PAYLOAD_BYTES..].copy_from_slice(&tag);
        Some(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)))
    }

    /// Malformed, expired and wrong-context proofs have one refusal result. The
    /// bounded decoder runs before a constant-time cryptographic MAC comparison.
    pub fn verify(&self, session: &SessionToken, binding: &PluginCsrfBinding<'_>, token: &str, now: u64) -> bool {
        if token.len() != PLUGIN_CSRF_TOKEN_BYTES || !binding.valid() {
            return false;
        }
        let Some(encoded) = token.strip_prefix(PREFIX) else { return false };
        let mut bytes = [0; DECODED_BYTES];
        if URL_SAFE_NO_PAD.decode_slice(encoded, &mut bytes) != Ok(DECODED_BYTES) {
            return false;
        }
        let issued = u64::from_be_bytes(bytes[..8].try_into().expect("fixed timestamp field"));
        if now.saturating_sub(issued) > PLUGIN_CSRF_MAX_AGE_SECONDS || issued.saturating_sub(now) > CLOCK_LEEWAY_SECONDS {
            return false;
        }
        self.mac(session, binding, &bytes[..PAYLOAD_BYTES]).verify_slice(&bytes[PAYLOAD_BYTES..]).is_ok()
    }

    fn mac(&self, session: &SessionToken, binding: &PluginCsrfBinding<'_>, payload: &[u8]) -> FormMac {
        let mut mac = <FormMac as KeyInit>::new_from_slice(&self.0).expect("HMAC accepts a 32-byte key");
        mac.update(b"rbit/plugin-form-csrf/POST/v1\0");
        mac.update(&session.digest());
        mac.update(&binding.actor_id.to_be_bytes());
        match binding.repository_id {
            Some(id) => {
                mac.update(&[1]);
                mac.update(&id.to_be_bytes());
            }
            None => mac.update(&[0]),
        }
        mac.update(&u16::try_from(binding.action.len()).expect("validated action length").to_be_bytes());
        mac.update(binding.action.as_bytes());
        mac.update(&binding.generation.to_be_bytes());
        mac.update(&binding.version.to_be_bytes());
        mac.update(payload);
        mac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> PluginCsrfBinding<'static> {
        PluginCsrfBinding {
            action: "/alice/project/-/plugins/example.forms/save",
            actor_id: 1,
            repository_id: Some(2),
            generation: 3,
            version: 4,
        }
    }

    #[test]
    fn randomized_proofs_verify_with_host_clones_without_exposing_secrets() {
        let key = PluginCsrfKey::generate();
        let session = SessionToken::generate();
        let first = key.issue(&session, &binding(), 1_000).unwrap();
        let second = key.issue(&session, &binding(), 1_000).unwrap();
        assert_eq!(first.len(), PLUGIN_CSRF_TOKEN_BYTES);
        assert_ne!(first, second);
        assert!(key.clone().verify(&session, &binding(), &first, 1_001));
        assert!(key.verify(&session, &binding(), &second, 1_001));
        assert_eq!(format!("{key:?}"), "PluginCsrfKey(<redacted>)");
        assert!(!first.contains(&session.encode()));
    }

    #[test]
    fn proofs_bind_every_authority_and_registration_claim() {
        let key = PluginCsrfKey::generate();
        let session = SessionToken::generate();
        let token = key.issue(&session, &binding(), 1_000).unwrap();
        assert!(!PluginCsrfKey::generate().verify(&session, &binding(), &token, 1_001));
        assert!(!key.verify(&SessionToken::generate(), &binding(), &token, 1_001));
        for changed in [
            PluginCsrfBinding { actor_id: 9, ..binding() },
            PluginCsrfBinding { repository_id: Some(9), ..binding() },
            PluginCsrfBinding { repository_id: None, ..binding() },
            PluginCsrfBinding { generation: 9, ..binding() },
            PluginCsrfBinding { version: 9, ..binding() },
            PluginCsrfBinding { action: "/alice/project/-/plugins/other.forms/save", ..binding() },
            PluginCsrfBinding { action: "/alice/project/-/plugins/example.forms/delete", ..binding() },
        ] {
            assert!(!key.verify(&session, &changed, &token, 1_001), "{changed:?}");
        }
    }

    #[test]
    fn expiry_clock_leeway_and_timestamp_extremes_fail_closed() {
        let key = PluginCsrfKey::generate();
        let session = SessionToken::generate();
        let token = key.issue(&session, &binding(), 1_000).unwrap();
        assert!(key.verify(&session, &binding(), &token, 1_000 + PLUGIN_CSRF_MAX_AGE_SECONDS));
        assert!(!key.verify(&session, &binding(), &token, 1_001 + PLUGIN_CSRF_MAX_AGE_SECONDS));
        assert!(key.verify(&session, &binding(), &token, 940));
        assert!(!key.verify(&session, &binding(), &token, 939));
        assert!(!key.verify(&session, &binding(), &token, u64::MAX));
        let future = key.issue(&session, &binding(), u64::MAX).unwrap();
        assert!(!key.verify(&session, &binding(), &future, 1_000));
    }

    #[test]
    fn malformed_oversized_and_tampered_tokens_are_refused() {
        let key = PluginCsrfKey::generate();
        let session = SessionToken::generate();
        let token = key.issue(&session, &binding(), 1_000).unwrap();
        for invalid in [String::new(), "x".repeat(100_000), format!("{token}="), token.replace("pf1.", "pf2."), "é".repeat(40)] {
            assert!(!key.verify(&session, &binding(), &invalid, 1_000));
        }
        let bytes = URL_SAFE_NO_PAD.decode(&token[PREFIX.len()..]).unwrap();
        for index in [0, 8, PAYLOAD_BYTES, DECODED_BYTES - 1] {
            let mut changed = bytes.clone();
            changed[index] ^= 1;
            let invalid = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(changed));
            assert!(!key.verify(&session, &binding(), &invalid, 1_000));
        }
    }

    #[test]
    fn invalid_host_bindings_never_get_signed() {
        let key = PluginCsrfKey::generate();
        let session = SessionToken::generate();
        for action in ["", "/", "//example.test/post", "/a?b", "/a#b", "/a%2fb", "/a/../b", "/a//b", "/a\\b"] {
            assert!(key.issue(&session, &PluginCsrfBinding { action, ..binding() }, 1_000).is_none());
        }
        for invalid in [
            PluginCsrfBinding { actor_id: 0, ..binding() },
            PluginCsrfBinding { repository_id: Some(0), ..binding() },
            PluginCsrfBinding { generation: 0, ..binding() },
            PluginCsrfBinding { version: 0, ..binding() },
        ] {
            assert!(key.issue(&session, &invalid, 1_000).is_none());
        }
    }
}
