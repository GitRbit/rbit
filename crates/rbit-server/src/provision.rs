//! `rbit user create`: offline account provisioning on this machine.
//!
//! The command owns only process concerns: the database handle, the password read from
//! standard input and a password-work owner local to this process. Name, email, password
//! policy, the first-administrator rule and the atomic insert are decided by the identity
//! registration application over the SQL account store, exactly as for sign-up.

use std::io::BufRead;

use anyhow::Context;
use rbit_core::{Config, User};
use rbit_identity::{
    PasswordWork,
    accounts::{AccountRegistration, SignUp},
};
use rbit_kernel::BlockingBudget;
use rbit_services::{ServiceError, accounts::registration_error};
use rbit_storage::AccountStore;
use zeroize::Zeroizing;

pub async fn create_user(config: Config, username: String, email: String, admin: bool) -> anyhow::Result<()> {
    let database = crate::serve::open_database(&config).await?;

    eprintln!("Password for {username} (read from stdin):");
    let mut line = Zeroizing::new(String::new());
    std::io::stdin().lock().read_line(&mut line).context("cannot read password")?;
    let password = Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_owned());
    drop(line);

    let user = provision(&config, &database.accounts(), username, email, password, admin).await?;
    tracing::info!(user = %user.name(), admin, "user created");
    database.close().await;
    Ok(())
}

/// Creates the account with an explicit administrator flag. The password-work owner is
/// local to this command; no other process work shares it.
async fn provision(
    config: &Config,
    accounts: &AccountStore,
    username: String,
    email: String,
    mut password: Zeroizing<String>,
    is_admin: bool,
) -> Result<User, ServiceError> {
    let capacity = config.security.password_job_capacity().map_err(ServiceError::Invalid)?;
    let passwords = PasswordWork::new(BlockingBudget::new(capacity));
    let application = AccountRegistration::new(accounts, &passwords, config.security.min_password_length, None);
    let input = SignUp { username, email, password: std::mem::take(&mut *password) };
    let user = application.create_user(input, Some(is_admin)).await.map_err(registration_error)?;
    tracing::info!(user = %user.name(), is_admin = user.is_admin, "account created");
    Ok(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> (tempfile::TempDir, rbit_storage::Database) {
        let dir = tempfile::tempdir().unwrap();
        let database = rbit_storage::Database::open(&dir.path().join("rbit.db")).await.unwrap();
        (dir, database)
    }

    #[tokio::test]
    async fn the_explicit_flag_decides_administrator_rights_and_registration_policy_applies() {
        let (_dir, database) = store().await;
        let config = Config::default();
        let accounts = database.accounts();
        let secret = || Zeroizing::new("correct horse battery".to_owned());
        let member = provision(&config, &accounts, "alice".into(), "alice@example.com".into(), secret(), false).await.unwrap();
        assert!(!member.is_admin, "an explicit flag overrides the first-administrator rule");
        let admin = provision(&config, &accounts, "bob".into(), "bob@example.com".into(), secret(), true).await.unwrap();
        assert!(admin.is_admin);
        let short = provision(&config, &accounts, "carol".into(), "carol@example.com".into(), Zeroizing::new("x".into()), false).await;
        assert!(short.is_err(), "the minimum password length still applies");
        let taken = provision(&config, &accounts, "alice".into(), "other@example.com".into(), secret(), false).await.unwrap_err();
        assert_ne!(taken.to_string(), "", "a duplicate name is refused with a message");
        database.close().await;
    }
}
