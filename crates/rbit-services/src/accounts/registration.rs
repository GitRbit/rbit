//! Account registration hashes before persistence and shares the instance password owner.

use rbit_core::{AccountName, Config, Email, User};
use rbit_identity::PasswordWork;
use rbit_storage::{AccountStore, NewUser};

use super::{MAX_PASSWORD_LEN, SignUp};
use crate::ServiceError;

/// Creates a user with only the relational database, for the `rbit user create`
/// command: it needs neither the key/value store (which a running server holds
/// exclusively) nor background services. Nothing in that process listens for
/// events, so none is published. `is_admin = None` applies the "first user is
/// admin" rule.
pub async fn create_user_offline(
    config: &Config,
    accounts: &AccountStore,
    input: SignUp,
    is_admin: Option<bool>,
) -> Result<User, ServiceError> {
    let passwords = crate::password_work::configured(config)?;
    create_user(config, accounts, &passwords, input, is_admin).await
}

/// Validates and stores a new user. Shared by sign-up, the CLI and the admin panel.
pub(crate) async fn create_user(
    config: &Config,
    accounts: &AccountStore,
    passwords: &PasswordWork,
    input: SignUp,
    is_admin: Option<bool>,
) -> Result<User, ServiceError> {
    let new = prepare_user(config, passwords, input, is_admin.unwrap_or(false)).await?;
    let user = if is_admin.is_some() { accounts.create_user(new).await? } else { accounts.register_user(new, true).await? };
    tracing::info!(user = %user.name(), is_admin = user.is_admin, "account created");
    Ok(user)
}

pub(crate) async fn prepare_user(
    config: &Config,
    passwords: &PasswordWork,
    input: SignUp,
    is_admin: bool,
) -> Result<NewUser, ServiceError> {
    let name: AccountName = input.username.trim().parse()?;
    let email: Email = input.email.trim().parse()?;
    validate_password(config, &input.password)?;

    let password_hash = passwords.begin()?.hash(input.password).await?;
    Ok(NewUser { name, email, display_name: None, password_hash, is_admin })
}

fn validate_password(config: &Config, password: &str) -> Result<(), ServiceError> {
    let min = config.security.min_password_length;
    let length = password.chars().count();
    if length < min {
        return Err(ServiceError::Invalid(format!("password must be at least {min} characters")));
    }
    if length > MAX_PASSWORD_LEN {
        return Err(ServiceError::Invalid(format!("password must be at most {MAX_PASSWORD_LEN} characters")));
    }
    Ok(())
}
