//! Identity-owned account models and explicitly layered credential functionality.
//!
//! Account models always compile with contracts/Serde only. The `credentials` feature
//! provides the legacy password facade; `password-work` adds explicitly injected bounded
//! work. Defaults retain the complete historical credential API. No feature creates a
//! global budget, performs startup or imports a concrete store/protocol implementation.

pub mod account;
pub mod accounts;
pub mod names;
pub mod sessions;
pub mod token;
#[cfg(feature = "session-tokens")]
pub use token::SessionToken;
pub use token::TokenDigest;

#[cfg(feature = "credentials")]
mod credential;
#[cfg(feature = "password-work")]
mod password_attempt;
#[cfg(feature = "password-work")]
mod password_work;

pub use account::{Account, AccountId, AccountKind, User};
#[cfg(feature = "credentials")]
pub use credential::{PasswordError, Passwords};
pub use names::{AccountName, Email};
#[cfg(feature = "password-work")]
pub use password_attempt::PasswordAttempt;
#[cfg(feature = "password-work")]
pub use password_work::PasswordWork;
