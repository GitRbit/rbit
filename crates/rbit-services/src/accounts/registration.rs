//! Legacy provisioning entry points delegate to the identity-owned application.

use rbit_core::{Config, User};
use rbit_identity::{PasswordWork, accounts::AccountRegistration};
use rbit_storage::AccountStore;

use super::{SignUp, application_error};
use crate::ServiceError;

/// SQL-only CLI path: its explicitly configured owner is local to this process.
pub fn create_user_offline<'a>(
    config: &'a Config,
    accounts: &'a AccountStore,
    input: SignUp,
    is_admin: Option<bool>,
) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
    create_user_offline_with::<false>(config, accounts, input, is_admin)
}

/// Explicit original provisioning; not selected by the existing offline CLI.
pub fn create_user_offline_original<'a>(
    config: &'a Config,
    accounts: &'a AccountStore,
    input: SignUp,
    is_admin: Option<bool>,
) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
    create_user_offline_with::<true>(config, accounts, input, is_admin)
}

fn create_user_offline_with<'a, const ORIGINAL: bool>(
    config: &'a Config,
    accounts: &'a AccountStore,
    input: SignUp,
    is_admin: Option<bool>,
) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
    // Eagerly move the input into protected application ownership even if configuration
    // fails or the caller drops an unpolled future. No owner is constructed implicitly.
    let passwords = crate::password_work::configured(config);
    let mut password = zeroize::Zeroizing::new(input.password);
    let input = SignUp { username: input.username, email: input.email, password: String::new() };
    async move {
        let passwords = passwords?;
        create_user_with::<ORIGINAL>(config, accounts, &passwords, SignUp { password: std::mem::take(&mut *password), ..input }, is_admin)
            .await
    }
}

pub(crate) fn create_user<'a>(
    config: &'a Config,
    accounts: &'a AccountStore,
    passwords: &'a PasswordWork,
    input: SignUp,
    is_admin: Option<bool>,
) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
    create_user_with::<false>(config, accounts, passwords, input, is_admin)
}

fn create_user_with<'a, const ORIGINAL: bool>(
    config: &'a Config,
    accounts: &'a AccountStore,
    passwords: &'a PasswordWork,
    input: SignUp,
    is_admin: Option<bool>,
) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
    let mut password = zeroize::Zeroizing::new(input.password);
    let input = SignUp { username: input.username, email: input.email, password: String::new() };
    async move {
        let application = AccountRegistration::new(accounts, passwords, config.security.min_password_length, None);
        let input = SignUp { password: std::mem::take(&mut *password), ..input };
        let user = if ORIGINAL {
            application.create_user_original(input, is_admin).await.map(|created| created.into_parts().0)
        } else {
            application.create_user(input, is_admin).await
        }
        .map_err(application_error::registration)?;
        tracing::info!(user = %user.name(), is_admin = user.is_admin, "account created");
        Ok(user)
    }
}

impl super::AccountService {
    /// Explicit original provisioning for the closed private composition only.
    pub fn create_user_original(
        &self,
        input: SignUp,
        is_admin: Option<bool>,
    ) -> impl Future<Output = Result<User, ServiceError>> + Send + '_ {
        let operation = create_user_with::<true>(&self.config, &self.accounts, &self.passwords, input, is_admin);
        async move {
            let user = operation.await?;
            self.events.publish(crate::DomainEvent::UserSignedUp { user: user.id() });
            Ok(user)
        }
    }

    /// Inactive sign-up producer for the eventual private fenced issuer. Current remote
    /// routes retain legacy sign-up until every issuance/recovery interlock is ready.
    pub fn sign_up_original<'a>(
        &'a self,
        input: SignUp,
        setup_token: &'a str,
    ) -> impl Future<Output = Result<super::ProvenAccount, ServiceError>> + Send + 'a {
        let operation = self
            .registration()
            .sign_up_original(input, setup_token, move || self.instance.current().registration == rbit_core::RegistrationMode::Open);
        async move {
            let proven = operation.await.map_err(application_error::registration)?;
            self.events.publish(crate::DomainEvent::UserSignedUp { user: proven.user().id() });
            Ok(proven)
        }
    }
}
