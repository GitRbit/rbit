//! Registration policy lives here; the repository retains atomic first-user authority.

use std::future::Future;

use super::{AccountError, AccountRepository, SignUp};
use crate::{PasswordWork, User};

mod input;
mod setup;
pub use setup::{SETUP_TOKEN_REQUIRED, bootstrap_setup_token};

pub(super) const MAX_PASSWORD_LEN: usize = 256;

pub struct AccountRegistration<'a, R> {
    repository: &'a R,
    passwords: &'a PasswordWork,
    minimum_password_length: usize,
    setup_token: Option<&'a str>,
}

#[derive(Clone, Copy)]
enum Provisioning {
    FirstAdministrator,
    Explicit(bool),
    InitialPassword(bool),
}

impl<'a, R: AccountRepository> AccountRegistration<'a, R> {
    pub fn new(repository: &'a R, passwords: &'a PasswordWork, minimum_password_length: usize, setup_token: Option<&'a str>) -> Self {
        Self { repository, passwords, minimum_password_length, setup_token }
    }

    pub async fn setup_required(&self) -> Result<bool, AccountError<R::Error>> {
        self.repository.count_users().await.map(|count| count == 0).map_err(AccountError::Store)
    }

    /// Preserve the live-open short circuit: an open instance does not count users.
    pub async fn registration_open(&self, live_open: bool) -> Result<bool, AccountError<R::Error>> {
        Ok(live_open || self.setup_required().await?)
    }

    /// Protect input before returning a future. Read live registration after the
    /// preflight count, but let the final SQL transaction arbitrate concurrent signups.
    pub fn sign_up(
        self,
        input: SignUp,
        given_setup_token: &'a str,
        live_registration_open: impl FnOnce() -> bool + Send + 'a,
    ) -> impl Future<Output = Result<User, AccountError<R::Error>>> + Send + 'a {
        let input = input::ProtectedSignUp::new(input);
        async move {
            let first = self.setup_required().await?;
            let open = live_registration_open();
            if !open && !first {
                return Err(AccountError::RegistrationClosed);
            }
            if first && !setup::token_matches(self.setup_token, given_setup_token) {
                return Err(AccountError::SetupTokenRequired);
            }
            let new = input.prepare(self.passwords, self.minimum_password_length, false).await?;
            self.repository.register_user(new, open).await.map_err(AccountError::Store)
        }
    }

    /// `None` delegates the first-administrator decision to the write transaction.
    pub fn create_user(
        self,
        input: SignUp,
        is_admin: Option<bool>,
    ) -> impl Future<Output = Result<User, AccountError<R::Error>>> + Send + 'a {
        self.create(input, is_admin.map_or(Provisioning::FirstAdministrator, Provisioning::Explicit))
    }

    /// For an already authorized administrator: SQL also marks password replacement required.
    pub fn create_user_with_initial_password(
        self,
        input: SignUp,
        is_admin: bool,
    ) -> impl Future<Output = Result<User, AccountError<R::Error>>> + Send + 'a {
        self.create(input, Provisioning::InitialPassword(is_admin))
    }

    fn create(self, input: SignUp, provisioning: Provisioning) -> impl Future<Output = Result<User, AccountError<R::Error>>> + Send + 'a {
        let input = input::ProtectedSignUp::new(input);
        async move {
            let is_admin = match provisioning {
                Provisioning::FirstAdministrator => false,
                Provisioning::Explicit(admin) | Provisioning::InitialPassword(admin) => admin,
            };
            let new = input.prepare(self.passwords, self.minimum_password_length, is_admin).await?;
            match provisioning {
                Provisioning::FirstAdministrator => self.repository.register_user(new, true).await,
                Provisioning::Explicit(_) => self.repository.create_user(new).await,
                Provisioning::InitialPassword(_) => self.repository.create_user_with_initial_password(new).await,
            }
            .map_err(AccountError::Store)
        }
    }
}

impl<R> std::fmt::Debug for AccountRegistration<'_, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountRegistration")
            .field("minimum_password_length", &self.minimum_password_length)
            .field("setup_token_present", &self.setup_token.is_some())
            .finish_non_exhaustive()
    }
}
