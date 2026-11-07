//! Feature-owned account persistence contract and password-work-gated applications.

pub use rbit_identity_models::accounts::{
    AccountCreationRepository, AccountRepository, CreatedAccountSnapshot, LinkedProviderSnapshot, LoginRecord, NewUser,
    ProviderAccountSnapshot, ProviderCreationRepository, ProviderLoginRepository, SignUp,
};

#[cfg(feature = "password-work")]
pub(crate) mod authentication;
#[cfg(feature = "password-work")]
mod error;
#[cfg(feature = "password-work")]
pub(crate) mod proof;
#[cfg(feature = "password-work")]
mod provider;
#[cfg(feature = "password-work")]
mod registration;
#[cfg(feature = "password-work")]
pub use proof::{OriginalProofReceipt, ProvenAccount};
#[cfg(feature = "password-work")]
pub use provider::{ProviderAuthentication, ProviderOrigin, ProviderProofError};

#[cfg(feature = "password-work")]
pub use authentication::{AuthenticationOutcome, PasswordAuthentication, PasswordProofError, PasswordProofOutcome};
#[cfg(feature = "password-work")]
pub use error::AccountError;
#[cfg(feature = "password-work")]
pub use registration::{AccountRegistration, SETUP_TOKEN_REQUIRED, bootstrap_setup_token};
