//! Sign-up, sign-in and session management.
//!
//! Suspended users are refused at every door this service guards: sign-in returns
//! [`ServiceError::Forbidden`] (only after the password has been verified, so suspension
//! is never revealed to someone who doesn't know the password) and sessions stop resolving.

use std::sync::Arc;

use rustit_auth::{Passwords, SessionToken};
use rustit_core::{Account, AccountId, AccountName, Config, Email, RegistrationMode, Timestamp, User};
use rustit_storage::{AccountStore, NewUser, SessionRecord, SessionStore};

use crate::{InstanceService, ServiceError};

/// Upper bound on password length: Argon2 accepts more, but unbounded input invites denial of service.
const MAX_PASSWORD_LEN: usize = 256;

/// Raw sign-up input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct SignUp {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct AccountService {
    config: Arc<Config>,
    accounts: AccountStore,
    sessions: SessionStore,
    instance: InstanceService,
}

impl AccountService {
    pub(crate) fn new(config: Arc<Config>, accounts: AccountStore, sessions: SessionStore, instance: InstanceService) -> Self {
        Self { config, accounts, sessions, instance }
    }

    /// Whether the public sign-up form is available (an instance setting administrators
    /// change at runtime). The very first account can always be created so that a fresh
    /// instance can be bootstrapped.
    pub async fn registration_open(&self) -> Result<bool, ServiceError> {
        Ok(self.instance.current().registration == RegistrationMode::Open || self.accounts.count_users().await? == 0)
    }

    /// Registers a user. The first user of an instance becomes its administrator.
    pub async fn sign_up(&self, input: SignUp) -> Result<User, ServiceError> {
        if !self.registration_open().await? {
            return Err(ServiceError::RegistrationClosed);
        }
        self.create_user(input, None).await
    }

    /// Creates a user regardless of the registration setting (admin CLI, admin panel).
    /// `is_admin = None` applies the "first user is admin" rule.
    pub async fn create_user(&self, input: SignUp, is_admin: Option<bool>) -> Result<User, ServiceError> {
        create_user(&self.config, &self.accounts, input, is_admin).await
    }

    /// Verifies credentials. Takes the same time whether or not the login exists.
    /// A suspended user with the right password gets [`ServiceError::Forbidden`].
    pub async fn authenticate(&self, login: &str, password: String) -> Result<User, ServiceError> {
        if password.len() > MAX_PASSWORD_LEN * 4 {
            return Err(ServiceError::InvalidCredentials);
        }
        let Some(record) = self.accounts.find_login(login.trim()).await? else {
            Passwords::verify_dummy(password).await;
            return Err(ServiceError::InvalidCredentials);
        };
        if !Passwords::verify(password, record.password_hash).await? {
            return Err(ServiceError::InvalidCredentials);
        }
        if record.suspended {
            tracing::info!(user = %record.user.name(), "suspended user refused sign-in");
            return Err(ServiceError::Forbidden);
        }
        Ok(record.user)
    }

    /// Whether an administrator has suspended the user. Credentials resolved elsewhere
    /// (personal access tokens) must be checked against this before they are honoured.
    pub async fn is_suspended(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.accounts.is_suspended(id).await?)
    }

    /// Opens a new session and returns the secret token for the cookie.
    pub async fn start_session(&self, user: &User) -> Result<SessionToken, ServiceError> {
        let token = SessionToken::generate();
        let record = SessionRecord { account_id: user.id(), expires_at: self.session_expiry() };
        self.sessions.put(token.digest(), record).await?;
        Ok(token)
    }

    /// Resolves a cookie token to its user, sliding the expiry forward on use. Sessions of
    /// deleted or suspended users are discarded.
    pub async fn resume_session(&self, token: &SessionToken) -> Result<Option<User>, ServiceError> {
        let digest = token.digest();
        let Some(mut record) = self.sessions.get(digest).await? else {
            return Ok(None);
        };
        let Some(user) = self.accounts.find_active_user(record.account_id).await? else {
            self.sessions.delete(digest).await?;
            return Ok(None);
        };

        // Only rewrite when at least a day has passed, to keep reads write-free.
        let renewed = self.session_expiry();
        if renewed.seconds() - record.expires_at.seconds() > 24 * 60 * 60 {
            record.expires_at = renewed;
            self.sessions.put(digest, record).await?;
        }
        Ok(Some(user))
    }

    pub async fn end_session(&self, token: &SessionToken) -> Result<(), ServiceError> {
        Ok(self.sessions.delete(token.digest()).await?)
    }

    pub fn session_lifetime_seconds(&self) -> i64 {
        i64::from(self.config.security.session_lifetime_days) * 24 * 60 * 60
    }

    fn session_expiry(&self) -> Timestamp {
        Timestamp(Timestamp::now().seconds() + self.session_lifetime_seconds())
    }

    pub async fn find_account(&self, name: &str) -> Result<Account, ServiceError> {
        self.accounts.find_account_by_name(name).await?.ok_or(ServiceError::NotFound)
    }
}

/// Validates and stores a new user. Shared by sign-up, the CLI and the admin panel.
pub(crate) async fn create_user(config: &Config, accounts: &AccountStore, input: SignUp, is_admin: Option<bool>) -> Result<User, ServiceError> {
    let name: AccountName = input.username.trim().parse()?;
    let email: Email = input.email.trim().parse()?;
    validate_password(config, &input.password)?;

    let is_admin = match is_admin {
        Some(flag) => flag,
        None => accounts.count_users().await? == 0,
    };
    let password_hash = Passwords::hash(input.password).await?;
    let user = accounts.create_user(NewUser { name, email, display_name: None, password_hash, is_admin }).await?;

    tracing::info!(user = %user.name(), is_admin, "account created");
    Ok(user)
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
