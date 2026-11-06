//! Sign-up, sign-in and session management.
//!
//! Suspended users are refused at every door this service guards: sign-in returns
//! [`ServiceError::Forbidden`] (only after the password has been verified, so suspension
//! is never revealed to someone who doesn't know the password) and sessions stop resolving.
//!
//! Sessions are indexed by account, so users can list them, revoke one or sign out
//! everywhere else. Credential finalization awaits session revocation before returning,
//! so enforcement cannot be lost on the event bus. Recovery and administrator resets end
//! every session. Self-service changes preserve only the initiating session, which the web
//! layer brackets with a [`PasswordChangeGuard`]. However active, a session ends
//! `session_max_age_days` after sign-in.
//!
//! The first account of an instance becomes its administrator, so signing it up needs a
//! setup token: the configured `setup_token`, or a random one generated (and logged by the
//! server) when the instance starts without any user.

use std::sync::Arc;

use rbit_core::{Account, AccountId, Config, RegistrationMode, User};
use rbit_identity::{PasswordWork, sessions::effects::PasswordSessionEnforcement};
pub use rbit_storage::SessionUser;
use rbit_storage::{AccountStore, SessionStore};

use crate::{DomainEvent, EventBus, InstanceService, ServiceError, audit::AuditService};

pub(crate) mod application_error;
pub(crate) mod effects;
pub use effects::PasswordChangeGuard;
mod password_authentication;
pub use password_authentication::OriginalProofOutcome;
pub use rbit_identity::accounts::{OriginalProofReceipt, ProvenAccount};
mod registration;
mod sessions;
pub use registration::create_user_offline;

pub use rbit_identity::accounts::SignUp;

pub use rbit_identity::sessions::{ActiveSession, ClientInfo};

#[derive(Debug, Clone)]
pub struct AccountService {
    passwords: PasswordWork,
    config: Arc<Config>,
    accounts: AccountStore,
    sessions: SessionStore,
    instance: InstanceService,
    events: EventBus,
    audit: AuditService,
    password_changes: Arc<PasswordSessionEnforcement<effects::PasswordSessionEffects>>,
    /// Required to sign up the first account; `None` when the instance started with users.
    setup_token: Option<Arc<str>>,
}

impl AccountService {
    /// Shares the session enforcement state with the services that change passwords.
    pub(crate) fn new(
        config: Arc<Config>,
        accounts: AccountStore,
        sessions: SessionStore,
        instance: InstanceService,
        events: EventBus,
        setup_token: Option<String>,
        passwords: PasswordWork,
    ) -> Self {
        let audit = AuditService::from_store(accounts.audit_log());
        let setup_token = setup_token.map(Arc::from);
        let password_changes = Arc::new(PasswordSessionEnforcement::new(effects::PasswordSessionEffects::new(
            accounts.clone(),
            sessions.clone(),
            audit.clone(),
        )));
        Self { passwords, config, accounts, sessions, instance, events, audit, password_changes, setup_token }
    }

    pub(crate) fn password_work(&self) -> &PasswordWork {
        &self.passwords
    }

    /// Whether the public sign-up form is available (an instance setting administrators
    /// change at runtime). The very first account can always be created so that a fresh
    /// instance can be bootstrapped.
    pub async fn registration_open(&self) -> Result<bool, ServiceError> {
        self.registration()
            .registration_open(self.instance.current().registration == RegistrationMode::Open)
            .await
            .map_err(application_error::registration)
    }

    /// Whether the instance has no user yet, so the next sign-up creates its administrator
    /// and needs the [setup token](Self::setup_token).
    pub async fn setup_required(&self) -> Result<bool, ServiceError> {
        self.registration().setup_required().await.map_err(application_error::registration)
    }

    /// The token the first account's sign-up needs, while the instance started without
    /// users (the server logs it). Also lets tests bootstrap an instance.
    pub fn setup_token(&self) -> Option<&str> {
        self.setup_token.as_deref()
    }

    /// Registers a user. The first user of an instance becomes its administrator, and
    /// needs `setup_token` (ignored for every later sign-up).
    pub fn sign_up<'a>(&'a self, input: SignUp, setup_token: &'a str) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
        let operation =
            self.registration().sign_up(input, setup_token, move || self.instance.current().registration == RegistrationMode::Open);
        async move {
            let user = operation.await.map_err(application_error::registration)?;
            self.events.publish(DomainEvent::UserSignedUp { user: user.id() });
            Ok(user)
        }
    }

    fn registration(&self) -> rbit_identity::accounts::AccountRegistration<'_, AccountStore> {
        rbit_identity::accounts::AccountRegistration::new(
            &self.accounts,
            &self.passwords,
            self.config.security.min_password_length,
            self.setup_token.as_deref(),
        )
    }

    /// Creates a user regardless of the registration setting (admin CLI, admin panel).
    /// `is_admin = None` applies the "first user is admin" rule.
    pub fn create_user(&self, input: SignUp, is_admin: Option<bool>) -> impl Future<Output = Result<User, ServiceError>> + Send + '_ {
        let operation = registration::create_user(&self.config, &self.accounts, &self.passwords, input, is_admin);
        async move {
            let user = operation.await?;
            self.events.publish(DomainEvent::UserSignedUp { user: user.id() });
            Ok(user)
        }
    }

    /// Whether an administrator has suspended the user. Credentials resolved elsewhere
    /// (personal access tokens) must be checked against this before they are honoured.
    pub async fn is_suspended(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.accounts.is_suspended(id).await?)
    }

    /// Restricts administrator-provisioned accounts until their initial password is replaced.
    pub async fn password_change_required(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.accounts.password_change_required(id).await?)
    }

    pub async fn find_account(&self, name: &str) -> Result<Account, ServiceError> {
        self.accounts.find_account_by_name(name).await?.ok_or(ServiceError::NotFound)
    }

    /// Current identity behind an old public profile name, if it has not been retaken.
    pub async fn moved_to(&self, name: &str) -> Result<Option<Account>, ServiceError> {
        Ok(self.accounts.find_by_old_name(name).await?)
    }
}

/// Resolves the exact feature-owned bootstrap policy using only the relational store.
pub(crate) async fn bootstrap_setup_token(config: &Config, accounts: &AccountStore) -> Result<Option<String>, ServiceError> {
    rbit_identity::accounts::bootstrap_setup_token(accounts, config.security.setup_token.as_deref())
        .await
        .map_err(application_error::registration)
}
