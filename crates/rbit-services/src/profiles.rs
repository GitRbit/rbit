//! User profile and account settings: the service adapter over the identity profile
//! application.
//!
//! Validation, password checks, throttle order and compare-and-set commits live in
//! `rbit_identity::profiles`. This adapter supplies the throttle, maps errors, logs,
//! audits, publishes domain events and finalizes sessions after a password change.

mod password_changes;

use rbit_core::{AccountId, Email, User};
use rbit_identity::profiles::{CredentialThrottle, ProfileApplication, ProfileError};
use rbit_storage::{AccountStore, StorageError, sqlite::profiles::ProfileStore};

use crate::audit::{AuditAction, AuditEntry, AuditService};
use crate::{AccountService, DomainEvent, ServiceContext, ServiceError, ThrottleScope, ThrottleService};

/// A user's editable profile. `None` means "not set".
pub type Profile = rbit_identity::profiles::AccountProfile;
/// The profile fields shown on `/{name}` in addition to the account's display name and bio.
pub use rbit_identity::profiles::PublicProfile;
/// Raw "edit profile" input, exactly as submitted. Blank fields clear the value.
pub type UpdateProfile = rbit_identity::profiles::ProfileUpdate;

#[derive(Debug, Clone)]
pub struct ProfileService {
    ctx: ServiceContext,
    accounts: AccountService,
    throttle: ThrottleService,
}

/// Account-keyed buckets of the shared throttle.
struct Throttle<'a>(&'a ThrottleService);

impl CredentialThrottle for Throttle<'_> {
    type Error = ServiceError;
    fn check_rename(&self, account: AccountId) -> Result<(), ServiceError> {
        self.0.check(ThrottleScope::AccountRename, &account.0.to_string())
    }
    fn check_password(&self, account: AccountId) -> Result<(), ServiceError> {
        self.0.check(ThrottleScope::PasswordCheck, &account.0.to_string())
    }
    fn record_password(&self, account: AccountId, valid: bool) {
        let key = account.0.to_string();
        if valid {
            self.0.reset(ThrottleScope::PasswordCheck, &key);
        } else {
            self.0.record_failure(ThrottleScope::PasswordCheck, &key);
        }
    }
}

type Application<'a> = ProfileApplication<'a, ProfileStore, AccountStore, Throttle<'a>>;

impl ProfileService {
    pub(crate) fn new(ctx: ServiceContext, accounts: AccountService, throttle: ThrottleService) -> Self {
        Self { ctx, accounts, throttle }
    }

    fn store(&self) -> ProfileStore {
        self.ctx.database.profiles()
    }

    fn application<'s>(&'s self, store: &'s ProfileStore, accounts: &'s AccountStore, throttle: &'s Throttle<'s>) -> Application<'s> {
        let min = self.ctx.config.security.min_password_length;
        ProfileApplication::new(store, accounts, throttle, self.accounts.password_work(), min)
    }

    /// The signed-in user's own profile, for the settings form.
    pub async fn profile(&self, user: &User) -> Result<Profile, ServiceError> {
        let (store, accounts, throttle) = (self.store(), self.ctx.database.accounts(), Throttle(&self.throttle));
        self.application(&store, &accounts, &throttle).profile(user).await.map_err(map)
    }

    /// Extras for an account's public profile page. Accounts without any are blank.
    pub async fn public_profile(&self, account_id: AccountId) -> Result<PublicProfile, ServiceError> {
        let (store, accounts, throttle) = (self.store(), self.ctx.database.accounts(), Throttle(&self.throttle));
        self.application(&store, &accounts, &throttle).public_profile(account_id).await.map_err(map)
    }

    /// Renames the authenticated user's own account without changing its identity or
    /// credentials. Old names redirect until reclaimed; they never become login aliases.
    pub fn change_username(
        &self,
        user: &User,
        current_password: String,
        new_name: &str,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut current_password = zeroize::Zeroizing::new(current_password);
        async move {
            let (store, accounts, throttle) = (self.store(), self.ctx.database.accounts(), Throttle(&self.throttle));
            let app = self.application(&store, &accounts, &throttle);
            let name = app.change_username(user, std::mem::take(&mut *current_password), new_name).await.map_err(map)?;
            tracing::info!(account = user.id().0, new_name = %name, "account renamed");
            AuditService::new(self.ctx.clone()).record(AuditEntry::new(AuditAction::UsernameChanged).by(user).on(name.to_string())).await;
            Ok(())
        }
    }

    /// Validates and replaces every profile field.
    pub async fn update_profile(&self, user: &User, input: UpdateProfile) -> Result<Profile, ServiceError> {
        let (store, accounts, throttle) = (self.store(), self.ctx.database.accounts(), Throttle(&self.throttle));
        let profile = self.application(&store, &accounts, &throttle).update_profile(user, input).await.map_err(map)?;
        tracing::info!(user = %user.name(), "profile updated");
        Ok(profile)
    }

    /// Changes the sign-in email after re-checking the current password.
    ///
    /// [`ServiceError::InvalidCredentials`] for a wrong password, [`ServiceError::Conflict`]
    /// if another account uses the address.
    pub fn change_email(
        &self,
        user: &User,
        current_password: String,
        new_email: &str,
    ) -> impl std::future::Future<Output = Result<Email, ServiceError>> {
        let mut current_password = zeroize::Zeroizing::new(current_password);
        async move {
            let (store, accounts, throttle) = (self.store(), self.ctx.database.accounts(), Throttle(&self.throttle));
            let app = self.application(&store, &accounts, &throttle);
            let email = app.change_email(user, std::mem::take(&mut *current_password), new_email).await.map_err(map)?;
            tracing::info!(user = %user.name(), "email changed");
            self.ctx.events.publish(DomainEvent::EmailChanged { user: user.id(), previous: user.email.to_string() });
            Ok(email)
        }
    }
}

fn map(error: ProfileError<StorageError, ServiceError>) -> ServiceError {
    map_with(error, |never| match never {})
}

fn map_with<R>(error: ProfileError<StorageError, ServiceError, R>, reserve: impl FnOnce(R) -> ServiceError) -> ServiceError {
    match error {
        ProfileError::Store(error) => error.into(),
        ProfileError::Throttle(error) => error,
        ProfileError::Reserve(error) => reserve(error),
        ProfileError::Rejected(rejection) => ServiceError::Invalid(rejection.to_string()),
        ProfileError::Name(error) => error.into(),
        ProfileError::InvalidCredentials => ServiceError::InvalidCredentials,
        ProfileError::NotFound => ServiceError::NotFound,
        ProfileError::Admission(error) => error.into(),
        ProfileError::Password(error) => error.into(),
    }
}
