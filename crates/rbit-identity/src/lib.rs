//! Identity-owned account models and explicitly layered credential functionality.
//!
//! Pure model ownership lives in `rbit-identity-models`, with compatibility reexports here.
//! Model consumers depend there directly to avoid application feature unification.
//! The `credentials` feature
//! provides the password error and credential inventory; `password-work` adds the only
//! password hashing path, explicitly injected bounded work. `access-tokens` adds the
//! personal access token application over static ports; `password-work` also carries the
//! self-service profile application, and with `session-tokens` the recovery application. `two-factor` adds TOTP, recovery codes and the
//! authenticator application; `passkeys` adds the `WebAuthn` ceremonies; `external` adds the OIDC flow and
//! ID-token checks. With `password-work`, `passkeys` also carries the second sign-in step.
//! No feature creates a
//! global budget, performs startup or imports a concrete store/protocol implementation.

#[cfg(feature = "access-tokens")]
pub mod access_tokens;
pub mod account;
pub mod accounts;
pub mod names;
#[cfg(feature = "external")]
pub mod oidc;
#[cfg(feature = "passkeys")]
pub mod passkeys;
#[cfg(feature = "password-work")]
pub mod profiles;
#[cfg(all(feature = "password-work", feature = "session-tokens"))]
pub mod recovery;
#[cfg(all(feature = "passkeys", feature = "password-work"))]
pub mod second_factor;
pub mod sessions;
pub mod token;
#[cfg(feature = "two-factor")]
pub mod two_factor;
pub use token::TokenDigest;
#[cfg(feature = "session-tokens")]
pub use token::{AccessTokenSecret, SessionToken};

#[cfg(feature = "credentials")]
mod credential;
#[cfg(feature = "password-work")]
mod password_attempt;
#[cfg(feature = "password-work")]
mod password_input;
#[cfg(feature = "password-work")]
mod password_work;
#[cfg(feature = "password-work")]
pub use password_input::{PasswordLength, classify_password_length};

pub use account::{Account, AccountId, AccountKind, User};
#[cfg(feature = "credentials")]
pub use credential::PasswordError;
pub use names::{AccountName, Email};
#[cfg(feature = "password-work")]
pub use password_attempt::PasswordAttempt;
#[cfg(feature = "password-work")]
pub use password_work::PasswordWork;

#[cfg(feature = "credentials")]
pub mod inventory;
