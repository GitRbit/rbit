//! Feature-owned account persistence contract and password-work-gated applications.

pub use rbit_identity_models::accounts::{AccountRepository, LoginRecord, NewUser, SignUp};

#[cfg(feature = "password-work")]
pub(crate) mod authentication;
#[cfg(feature = "password-work")]
mod error;
#[cfg(feature = "password-work")]
mod registration;

#[cfg(feature = "password-work")]
pub use authentication::{
    AuthenticationOutcome, OriginalProofReceipt, PasswordAuthentication, PasswordProofError, PasswordProofOutcome, ProvenAccount,
};
#[cfg(feature = "password-work")]
pub use error::AccountError;
#[cfg(feature = "password-work")]
pub use registration::{AccountRegistration, SETUP_TOKEN_REQUIRED, bootstrap_setup_token};
