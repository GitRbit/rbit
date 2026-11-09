//! # rbit-auth
//!
//! Security primitives with no knowledge of HTTP or storage:
//!
//! * [`session`] — unguessable session tokens; only their digests are ever stored.
//! * [`access_token`] — personal access token secrets (`rbit_…`; legacy `rit_…`), likewise stored as digests.
//! * [`policy`] — compatibility path for repository access, owned by `rbit-access`.
//! * [`totp`] — time-based one-time passwords (RFC 6238) and recovery codes.
//! * [`webauthn`] — passkeys and security keys as a second factor.
//! * [`oidc`] — external sign-in: sealed flow state, PKCE and ID token validation.
//! * [`runner_token`], [`task_token`] — rbit Actions runner credentials (`rbrn_…`),
//!   registration tokens (`rbrg_…`) and task tokens (`rbat_…`), stored as digests.
//!
//! Flow state, ID-token checks/claims and newly registered credential Debug output
//! is opaque. Explicit field access and wire/cryptographic representations remain
//! caller-owned data and are not redacted by this diagnostic boundary.

pub mod access_token;
pub mod oidc;
/// Repository access compatibility path: the rules and grant loading are owned by
/// `rbit_access::grants`; `rbit_core::access` projects the domain records onto them.
pub mod policy {
    pub use rbit_core::access::{Grants, repo_access};
}
// Actions: S.
pub mod runner_token;
mod secret;
pub mod session;
// Actions: S.
pub mod task_token;
pub mod totp;
pub mod webauthn;

pub use access_token::AccessTokenSecret;
pub use oidc::{Flow, FlowKey, IdToken, IdTokenCheck, JwkSet, OidcError, Pkce};
pub use policy::{Grants, repo_access};
// Actions: S.
pub use runner_token::{RegistrationSecret, RunnerCredential};
pub use secret::TokenDigest;
pub use session::SessionToken;
// Actions: S.
pub use task_token::TaskTokenSecret;
pub use totp::{RecoveryCode, TotpSecret};
pub use webauthn::{Challenge, NewCredential, RelyingParty, WebauthnError};
