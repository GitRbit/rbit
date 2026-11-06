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
    // Eagerly move the input into protected application ownership even if configuration
    // fails or the caller drops an unpolled future. No owner is constructed implicitly.
    let passwords = crate::password_work::configured(config);
    let mut password = zeroize::Zeroizing::new(input.password);
    let input = SignUp { username: input.username, email: input.email, password: String::new() };
    async move {
        let passwords = passwords?;
        create_user(config, accounts, &passwords, SignUp { password: std::mem::take(&mut *password), ..input }, is_admin).await
    }
}

pub(crate) fn create_user<'a>(
    config: &'a Config,
    accounts: &'a AccountStore,
    passwords: &'a PasswordWork,
    input: SignUp,
    is_admin: Option<bool>,
) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
    let operation = AccountRegistration::new(accounts, passwords, config.security.min_password_length, None).create_user(input, is_admin);
    async move {
        let user = operation.await.map_err(application_error::registration)?;
        tracing::info!(user = %user.name(), is_admin = user.is_admin, "account created");
        Ok(user)
    }
}
