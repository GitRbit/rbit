//! Identity credential primitives, independent of forge state and protocol adapters.
//!
//! Password algorithm and error compatibility live here so future identity services
//! need not depend on the legacy authentication-policy crate. HTTP, storage, user
//! records and configuration remain outside this boundary. [`PasswordWork`] receives
//! its capacity explicitly from composition; [`Passwords`] retains the legacy direct
//! Tokio scheduling API until its callers are migrated. No global/default budget exists.

mod credential;
mod password_attempt;
mod password_work;

pub use credential::{PasswordError, Passwords};
pub use password_attempt::PasswordAttempt;
pub use password_work::PasswordWork;
