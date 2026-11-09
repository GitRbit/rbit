//! Building blocks for signing in with an external identity provider (OIDC and
//! GitHub's OAuth web flow), with no HTTP or storage:
//!
//! * [`FlowKey`] — keyed with a server secret, it seals the short-lived flow cookie
//!   ([`Flow`]: provider, `state`, where to go next, which account is linking) with
//!   HMAC-SHA256, and derives the flow's PKCE verifier and OIDC `nonce` from its random
//!   `state`, so neither has to be stored anywhere. It also issues the per-session token
//!   that authorises starting a link from the settings page.
//! * [`Pkce`] — RFC 7636 with `S256` only.
//! * [`JwkSet`] and [`verify_id_token`] — ID token validation: compact JWS with `RS256`
//!   (RSA ≥ 2048 bits) or `ES256` only (`none`, `HS*` and everything else are refused),
//!   the key chosen by `kid` (an unknown `kid` is reported as [`OidcError::UnknownKey`] so
//!   the caller can fetch the provider's keys again), then `iss`, `aud` (with `azp` when
//!   there are several audiences), `exp`, `iat` and `nbf` with [`CLOCK_LEEWAY_SECONDS`],
//!   and the `nonce`.
//!
//! Milestone 3 workstream O (external sign-in) owns this file.

mod error;
mod flow;
mod tokens;

pub use self::error::OidcError;
pub use self::flow::{Flow, FlowKey, Pkce};
pub use self::tokens::{IdToken, IdTokenCheck, JwkSet, token_key_id, verify_id_token};

/// Clock skew tolerated between us and the provider.
pub const CLOCK_LEEWAY_SECONDS: i64 = 60;

#[cfg(test)]
mod tests;
