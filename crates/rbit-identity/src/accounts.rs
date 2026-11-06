//! Feature-owned account persistence contract and password-work-gated applications.

mod records;
mod repository;

pub use records::{LoginRecord, NewUser, SignUp};
pub use repository::AccountRepository;

#[cfg(feature = "password-work")]
mod authentication;
#[cfg(feature = "password-work")]
mod error;
#[cfg(feature = "password-work")]
mod registration;

#[cfg(feature = "password-work")]
pub use authentication::{AuthenticationOutcome, PasswordAuthentication};
#[cfg(feature = "password-work")]
pub use error::AccountError;
#[cfg(feature = "password-work")]
pub use registration::{AccountRegistration, SETUP_TOKEN_REQUIRED, bootstrap_setup_token};
