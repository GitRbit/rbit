//! # rustit-auth
//!
//! Security primitives with no knowledge of HTTP or storage:
//!
//! * [`password`] — Argon2id hashing tuned to OWASP recommendations.
//! * [`session`] — unguessable session tokens; only their digests are ever stored.
//! * [`access_token`] — personal access token secrets (`rit_…`), likewise stored as digests.
//! * [`policy`] — who may read, write or administer a repository.
//! * [`totp`] — time-based one-time passwords (RFC 6238) and recovery codes.

pub mod access_token;
pub mod password;
pub mod policy;
mod secret;
pub mod session;
pub mod totp;

pub use access_token::AccessTokenSecret;
pub use password::{PasswordError, Passwords};
pub use policy::repo_access;
pub use secret::TokenDigest;
pub use session::SessionToken;
pub use totp::{RecoveryCode, TotpSecret};
