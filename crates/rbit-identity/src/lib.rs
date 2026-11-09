//! Identity-owned account models and explicitly layered credential functionality.
//!
//! Pure model ownership lives in `rbit-identity-models`, with compatibility reexports here.
//! Model consumers depend there directly to avoid application feature unification.
//! The `credentials` feature
//! provides the password error and credential inventory; `password-work` adds the only
//! password hashing path, explicitly injected bounded work. `access-tokens` adds the
//! personal access token application over static ports. No feature creates a
//! global budget, performs startup or imports a concrete store/protocol implementation.

#[cfg(feature = "access-tokens")]
pub mod access_tokens;
pub mod account;
pub mod accounts;
pub mod names;
pub mod sessions;
pub mod token;
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
