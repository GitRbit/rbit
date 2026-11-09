//! Self-service profile application: profile fields, email, username and password changes.
//!
//! Policy lives here, independent of SQL, HTTP, audit and rate-limit storage: field
//! validation, new-password policy, the order of throttle checks, password verification
//! through admitted [`PasswordWork`], and compare-and-set commits that lose to a concurrent
//! reset or change. Callers supply the throttle and keep logging, audit, domain events and
//! session finalization after a password change.

mod credentials;
mod fields;
mod rejection;

pub use credentials::{PreparedPasswordChange, commit_password_change};
pub use rbit_identity_models::profiles::{AccountProfile, AccountRenameRepository, ProfileRepository, PublicProfile};
pub use rejection::ProfileRejection;

use crate::{PasswordError, PasswordWork, User};
use rbit_contracts::{AccountId, NameError};
use rbit_kernel::AdmissionError;

/// Upper bound on password length: Argon2 accepts more, but unbounded input invites denial of service.
pub const MAX_PASSWORD_LEN: usize = 256;

/// Raw "edit profile" input, exactly as submitted. Blank fields clear the value.
#[derive(Debug, Clone, Default)]
pub struct ProfileUpdate {
    pub display_name: String,
    pub bio: String,
    pub website: String,
    pub location: String,
}

/// Per-account attempt limits. Implementations own buckets and their storage; the
/// application decides when each is checked, charged or cleared.
pub trait CredentialThrottle: Sync {
    type Error: Send;
    fn check_rename(&self, account: AccountId) -> Result<(), Self::Error>;
    fn check_password(&self, account: AccountId) -> Result<(), Self::Error>;
    /// A failed check is charged; a successful one clears the account's password bucket.
    fn record_password(&self, account: AccountId, valid: bool);
}

#[derive(Debug)]
pub enum ProfileError<Store, Throttle, Reserve = std::convert::Infallible> {
    Store(Store),
    Throttle(Throttle),
    /// The caller's session-finalization reservation was refused.
    Reserve(Reserve),
    /// The rejection's text is user-facing.
    Rejected(ProfileRejection),
    Name(NameError),
    InvalidCredentials,
    NotFound,
    Admission(AdmissionError),
    Password(PasswordError),
}

pub type ProfileResult<T, Store, Throttle, Reserve = std::convert::Infallible> = Result<T, ProfileError<Store, Throttle, Reserve>>;

#[derive(Debug)]
pub struct ProfileApplication<'a, P, A, T> {
    profiles: &'a P,
    accounts: &'a A,
    throttle: &'a T,
    passwords: &'a PasswordWork,
    min_password_length: usize,
}

impl<'a, P, A, T> ProfileApplication<'a, P, A, T>
where
    P: ProfileRepository,
    A: AccountRenameRepository<Error = P::Error>,
    T: CredentialThrottle,
{
    pub fn new(profiles: &'a P, accounts: &'a A, throttle: &'a T, passwords: &'a PasswordWork, min_password_length: usize) -> Self {
        Self { profiles, accounts, throttle, passwords, min_password_length }
    }

    /// The signed-in user's own profile, for the settings form.
    pub async fn profile(&self, user: &User) -> ProfileResult<AccountProfile, P::Error, T::Error> {
        self.profiles.find_profile(user.id()).await.map_err(ProfileError::Store)?.ok_or(ProfileError::NotFound)
    }

    /// Extras for an account's public profile page. Accounts without any are blank.
    pub async fn public_profile(&self, account: AccountId) -> ProfileResult<PublicProfile, P::Error, T::Error> {
        let profile = self.profiles.find_profile(account).await.map_err(ProfileError::Store)?.unwrap_or_default();
        Ok(PublicProfile { website: profile.website, location: profile.location })
    }

    /// Validates and replaces every profile field.
    pub async fn update_profile(&self, user: &User, input: ProfileUpdate) -> ProfileResult<AccountProfile, P::Error, T::Error> {
        let profile = fields::validate(&input).map_err(ProfileError::Rejected)?;
        self.profiles.update_profile(user.id(), &profile).await.map_err(ProfileError::Store)?;
        Ok(profile)
    }
}
