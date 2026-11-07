//! Sealed login flow state, PKCE derivation and per-session link authorization.

use super::CLOCK_LEEWAY_SECONDS;
use crate::secret::random_bytes;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use subtle::ConstantTimeEq;

/// Shortest server key accepted.
const MIN_KEY_LEN: usize = 32;
/// Prefix of sealed flow cookies, so the format can change later.
const FLOW_VERSION: &str = "v1";
type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------- PKCE

/// A PKCE code verifier (RFC 7636): 43 characters of base64url.
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce(String);

impl fmt::Debug for Pkce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pkce(<redacted>)")
    }
}

impl Pkce {
    /// The only method offered.
    pub const METHOD: &'static str = "S256";

    fn from_bytes(bytes: &[u8]) -> Self {
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Sent with the token request.
    pub fn verifier(&self) -> &str {
        &self.0
    }

    /// Sent with the authorization request: base64url(SHA-256(verifier)).
    pub fn challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(self.0.as_bytes()))
    }
}

// ---------------------------------------------------------------------- the flow cookie

/// One sign-in (or link) in progress, carried by the browser in a sealed cookie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    /// The provider id the flow was started for.
    #[serde(rename = "p")]
    pub provider: String,
    /// The random `state` sent to the provider (base64url, 256 bits).
    #[serde(rename = "s")]
    pub state: String,
    /// Where to go afterwards (already checked to be a local path).
    #[serde(rename = "n")]
    pub next: String,
    /// The signed-in account that is linking this provider, if this is a link.
    #[serde(rename = "l", default, skip_serializing_if = "Option::is_none")]
    pub link_account: Option<i64>,
    /// Unix seconds.
    #[serde(rename = "t")]
    pub issued_at: i64,
}

impl Flow {
    /// A new flow with a fresh random `state`.
    pub fn new(provider: &str, next: &str, link_account: Option<i64>, now: i64) -> Self {
        Self {
            provider: provider.to_owned(),
            state: URL_SAFE_NO_PAD.encode(random_bytes()),
            next: next.to_owned(),
            link_account,
            issued_at: now,
        }
    }

    /// Whether the `state` the provider sent back is this flow's, in constant time.
    pub fn state_matches(&self, returned: &str) -> bool {
        self.state.len() == returned.len() && bool::from(self.state.as_bytes().ct_eq(returned.as_bytes()))
    }
}

/// The server key that seals flow cookies and derives per-flow secrets.
#[derive(Clone)]
pub struct FlowKey([u8; 32]);

impl fmt::Debug for FlowKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FlowKey(<redacted>)")
    }
}

impl FlowKey {
    /// `None` for keys shorter than 32 bytes. Longer keys are compressed with SHA-256.
    pub fn new(secret: &[u8]) -> Option<Self> {
        (secret.len() >= MIN_KEY_LEN).then(|| Self(Sha256::digest(secret).into()))
    }

    /// HMAC-SHA256 over `label`, a NUL and `data`: each purpose gets its own label, so
    /// a value made for one can never be replayed as another.
    fn mac(&self, label: &str, data: &[u8]) -> [u8; 32] {
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(&self.0).unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
        mac.update(label.as_bytes());
        mac.update(&[0]);
        mac.update(data);
        mac.finalize().into_bytes().into()
    }

    /// The cookie value: `v1.<payload>.<mac>`, both base64url.
    pub fn seal(&self, flow: &Flow) -> String {
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(flow).unwrap_or_default());
        let mac = URL_SAFE_NO_PAD.encode(self.mac("rbit/oauth-flow/v1", payload.as_bytes()));
        format!("{FLOW_VERSION}.{payload}.{mac}")
    }

    /// The flow sealed in `value` if the MAC checks out and it is at most `max_age`
    /// seconds old (and not from the future beyond the clock leeway).
    pub fn open(&self, value: &str, now: i64, max_age: i64) -> Option<Flow> {
        let rest = value.strip_prefix(FLOW_VERSION)?.strip_prefix('.')?;
        let (payload, mac) = rest.split_once('.')?;
        let mac = URL_SAFE_NO_PAD.decode(mac).ok()?;
        let expected = self.mac("rbit/oauth-flow/v1", payload.as_bytes());
        if !bool::from(expected.as_slice().ct_eq(&mac)) {
            return None;
        }
        let flow: Flow = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
        let age = now - flow.issued_at;
        ((-CLOCK_LEEWAY_SECONDS..=max_age).contains(&age)).then_some(flow)
    }

    /// The flow's PKCE verifier, derived from its `state`.
    pub fn pkce(&self, flow: &Flow) -> Pkce {
        Pkce::from_bytes(&self.mac("rbit/oauth-pkce/v1", flow.state.as_bytes()))
    }

    /// The flow's OIDC `nonce`, derived from its `state`.
    pub fn nonce(&self, flow: &Flow) -> String {
        URL_SAFE_NO_PAD.encode(self.mac("rbit/oauth-nonce/v1", flow.state.as_bytes()))
    }

    /// A token for `/login/{provider}?link=…` bound to one session (by the digest of its
    /// token), so another site cannot start a link on a signed-in user's behalf.
    pub fn link_token(&self, session_digest: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(&self.mac("rbit/oauth-link/v1", session_digest)[..16])
    }

    /// Checks a [`Self::link_token`] in constant time.
    pub fn check_link_token(&self, session_digest: &[u8], token: &str) -> bool {
        let expected = self.link_token(session_digest);
        expected.len() == token.len() && bool::from(expected.as_bytes().ct_eq(token.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
