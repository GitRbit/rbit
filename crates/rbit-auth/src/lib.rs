//! # rbit-auth
//!
//! Security primitives with no knowledge of HTTP or storage:
//!
//! * [`password`] — Argon2id hashing tuned to OWASP recommendations.
//! * [`session`] — unguessable session tokens; only their digests are ever stored.
//! * [`access_token`] — personal access token secrets (`rbit_…`; legacy `rit_…`), likewise stored as digests.
//! * [`policy`] — who may read, write or administer a repository.
//! * [`totp`] — time-based one-time passwords (RFC 6238) and recovery codes.
//! * [`webauthn`] — passkeys and security keys as a second factor.
//! * [`oidc`] — external sign-in: sealed flow state, PKCE and ID token validation.

pub mod access_token;
pub mod oidc;
pub mod password;
pub mod policy;
mod secret;
pub mod session;
pub mod totp;
pub mod webauthn;

pub use access_token::AccessTokenSecret;
pub use oidc::{Flow, FlowKey, IdToken, IdTokenCheck, JwkSet, OidcError, Pkce};
pub use password::{PasswordError, Passwords};
pub use policy::{Grants, repo_access};
pub use secret::TokenDigest;
pub use session::SessionToken;
pub use totp::{RecoveryCode, TotpSecret};
pub use webauthn::{Challenge, NewCredential, RelyingParty, WebauthnError};
