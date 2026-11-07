//! Registration policy lives here; the repository retains atomic first-user authority.

use std::future::Future;

use super::{AccountCreationRepository, AccountError, AccountRepository, NewUser, ProvenAccount, SignUp, proof::creation_proven};
use crate::{PasswordWork, User};

mod input;
mod setup;
pub use setup::{SETUP_TOKEN_REQUIRED, bootstrap_setup_token};

pub(super) const MAX_PASSWORD_LEN: usize = 256;

type PreparedSignUp<Store> = Result<(NewUser, bool), AccountError<Store>>;

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
        let repository = self.repository;
        let preparation = self.prepare_sign_up(input, given_setup_token, live_registration_open);
        async move {
            let (new, open) = preparation.await?;
            repository.register_user(new, open).await.map_err(AccountError::Store)
        }
    }

    /// Inactive creation-proof counterpart. Validation/setup/admission are shared with
    /// legacy sign-up; its snapshot must be returned by the insertion transaction.
    pub fn sign_up_original(
        self,
        input: SignUp,
        given_setup_token: &'a str,
        live_registration_open: impl FnOnce() -> bool + Send + 'a,
    ) -> impl Future<Output = Result<ProvenAccount, AccountError<R::Error>>> + Send + 'a
    where
        R: AccountCreationRepository,
    {
        let repository = self.repository;
        let preparation = self.prepare_sign_up(input, given_setup_token, live_registration_open);
        async move {
            let (new, open) = preparation.await?;
            repository.register_user_original(new, open).await.map(creation_proven).map_err(AccountError::Store)
        }
    }

    fn prepare_sign_up(
        self,
        input: SignUp,
        given_setup_token: &'a str,
        live_registration_open: impl FnOnce() -> bool + Send + 'a,
    ) -> impl Future<Output = PreparedSignUp<R::Error>> + Send + 'a {
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
            Ok((new, open))
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

    /// Original provisioning shares validation and hashing with the legacy entry.
    /// Only a successful creation transaction can mint its proof.
    pub fn create_user_original(
        self,
        input: SignUp,
        is_admin: Option<bool>,
    ) -> impl Future<Output = Result<ProvenAccount, AccountError<R::Error>>> + Send + 'a
    where
        R: AccountCreationRepository,
    {
        self.create_original(input, is_admin.map_or(Provisioning::FirstAdministrator, Provisioning::Explicit))
    }

    pub fn create_user_with_initial_password_original(
        self,
        input: SignUp,
        is_admin: bool,
    ) -> impl Future<Output = Result<ProvenAccount, AccountError<R::Error>>> + Send + 'a
    where
        R: AccountCreationRepository,
    {
        self.create_original(input, Provisioning::InitialPassword(is_admin))
    }

    fn prepare_creation(
        self,
        input: SignUp,
        provisioning: Provisioning,
    ) -> impl Future<Output = Result<NewUser, AccountError<R::Error>>> + Send + 'a {
        let input = input::ProtectedSignUp::new(input);
        async move {
            let is_admin = match provisioning {
                Provisioning::FirstAdministrator => false,
                Provisioning::Explicit(admin) | Provisioning::InitialPassword(admin) => admin,
            };
            input.prepare(self.passwords, self.minimum_password_length, is_admin).await
        }
    }

    fn create(self, input: SignUp, provisioning: Provisioning) -> impl Future<Output = Result<User, AccountError<R::Error>>> + Send + 'a {
        let repository = self.repository;
        let preparation = self.prepare_creation(input, provisioning);
        async move {
            let new = preparation.await?;
            match provisioning {
                Provisioning::FirstAdministrator => repository.register_user(new, true).await,
                Provisioning::Explicit(_) => repository.create_user(new).await,
                Provisioning::InitialPassword(_) => repository.create_user_with_initial_password(new).await,
            }
            .map_err(AccountError::Store)
        }
    }
    fn create_original(
        self,
        input: SignUp,
        provisioning: Provisioning,
    ) -> impl Future<Output = Result<ProvenAccount, AccountError<R::Error>>> + Send + 'a
    where
        R: AccountCreationRepository,
    {
        let repository = self.repository;
        let preparation = self.prepare_creation(input, provisioning);
        async move {
            let new = preparation.await?;
            match provisioning {
                Provisioning::FirstAdministrator => repository.register_user_original(new, true).await,
                Provisioning::Explicit(_) => repository.create_user_original(new).await,
                Provisioning::InitialPassword(_) => repository.create_user_with_initial_password_original(new).await,
            }
            .map(creation_proven)
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
