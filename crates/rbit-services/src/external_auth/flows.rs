//! Completion and registration adapters: `rbit_identity::external` decides what a verified
//! identity does; these run its port over the stores, audit, two-factor and events, for
//! the legacy and the inactive fixed-origin producers alike.
use std::marker::PhantomData;

use rbit_identity::external::{
    VerifiedIdentity,
    completion::{self, AuditKind, Completion, CompletionError, ExternalAccounts, ExternalAudit},
    registration::{self, CreateFailure, ExternalRegistrations, NewExternalAccount, RegistrationError},
};
use rbit_storage::sqlite::external_identities::ExternalIdentityStore;

use super::original::{Legacy, Mode, Original, OriginalExternalSignIn};
use super::{
    AccountId, AuditAction, AuditEntry, Callback, ClientInfo, DomainEvent, ExternalIdentity, ExternalSignIn, LinkConflict,
    NewExternalIdentity, NewUser, ServiceError, Services, SessionToken, StorageError, ThrottleScope, Timestamp, User, ip_key, policy,
};

impl Services {
    /// Decides what a verified callback means — sign in, second step, link, register or
    /// refuse — following `rbit_identity::external::completion`.
    pub async fn complete_external_sign_in(&self, callback: Callback, client: &ClientInfo) -> Result<ExternalSignIn, ServiceError> {
        Ok(match self.complete_external_with::<Legacy>(callback, client).await? {
            Completion::SignedIn(user) => ExternalSignIn::SignedIn(user),
            Completion::SecondFactorRequired(token) => ExternalSignIn::SecondFactorRequired(token),
            Completion::Linked => ExternalSignIn::Linked,
            Completion::Register(token) => ExternalSignIn::Register(token),
        })
    }

    /// Inactive fixed-origin counterpart. Production callback routes stay on the legacy
    /// API until the complete writer/version/recovery interlocks are activated.
    pub async fn complete_external_sign_in_original(
        &self,
        callback: Callback,
        client: &ClientInfo,
    ) -> Result<OriginalExternalSignIn, ServiceError> {
        // A sealed legacy bare ID is not original-session link authority.
        if callback.link_account.is_some() {
            return Err(ServiceError::Forbidden);
        }
        Ok(match self.complete_external_with::<Original>(callback, client).await? {
            Completion::SignedIn(proven) => OriginalExternalSignIn::SignedIn(proven),
            Completion::SecondFactorRequired(token) => OriginalExternalSignIn::SecondFactorRequired(token),
            Completion::Linked => OriginalExternalSignIn::Linked,
            Completion::Register(token) => OriginalExternalSignIn::Register(token),
        })
    }

    async fn complete_external_with<M: Mode>(
        &self,
        callback: Callback,
        client: &ClientInfo,
    ) -> Result<Completion<M::Account>, ServiceError> {
        let Callback { identity, next, link_account } = callback;
        let config = self.external_auth.provider(&identity.provider_id).ok_or(ServiceError::NotFound)?.config.clone();
        let port = ModeAccounts::<M>::new(self, client, &config.id, None);
        completion::complete(&port, &policy(&config), identity, next, link_account).await.map_err(|error| match error {
            CompletionError::Refused(message) => ServiceError::Invalid(message),
            CompletionError::Forbidden => ServiceError::Forbidden,
            CompletionError::Port(error) => error,
        })
    }

    /// Finishes the "choose a username" step: creates the account (without a password)
    /// linked to the identity and signs it in. `email` must be an address; it is marked
    /// verified only if it is the provider's verified one. Throttled like sign-up.
    pub async fn register_external(
        &self,
        provider_id: &str,
        token: &SessionToken,
        username: &str,
        email: &str,
        client: &ClientInfo,
    ) -> Result<(User, String), ServiceError> {
        self.register_external_with::<Legacy>(provider_id, token, username, email, client).await
    }

    /// Inactive creation counterpart; the original epoch comes from the same
    /// account/identity transaction, never a subsequent account lookup.
    pub async fn register_external_original(
        &self,
        provider_id: &str,
        token: &SessionToken,
        username: &str,
        email: &str,
        client: &ClientInfo,
    ) -> Result<(rbit_identity::accounts::ProvenAccount, String), ServiceError> {
        self.register_external_with::<Original>(provider_id, token, username, email, client).await
    }

    async fn register_external_with<M: Mode>(
        &self,
        provider_id: &str,
        token: &SessionToken,
        username: &str,
        email: &str,
        client: &ClientInfo,
    ) -> Result<(M::Account, String), ServiceError> {
        let external = &self.external_auth;
        self.throttle.check(ThrottleScope::SignUp, &ip_key(client))?;
        let pending = external
            .pending_registration(token)
            .filter(|pending| pending.identity.provider_id == provider_id)
            .ok_or(ServiceError::NotFound)?;
        let config = external.provider(provider_id).ok_or(ServiceError::NotFound)?.config.clone();
        let port = ModeAccounts::<M>::new(self, client, provider_id, Some(token));
        let account =
            registration::register(&port, &policy(&config), &pending.identity, username, email).await.map_err(|error| match error {
                RegistrationError::Closed => ServiceError::RegistrationClosed,
                RegistrationError::Name(error) => error.into(),
                RegistrationError::Refused(message) => ServiceError::Invalid(message),
                RegistrationError::Port(error) => error,
            })?;
        Ok((account, pending.next))
    }
}

/// The external sign-in port for one request, over one mode's lookups.
struct ModeAccounts<'a, M> {
    services: &'a Services,
    client: &'a ClientInfo,
    store: ExternalIdentityStore,
    /// One timestamp for every write of the request.
    now: Timestamp,
    provider_id: &'a str,
    /// The "choose a username" step being finished, if any.
    registration: Option<&'a SessionToken>,
    mode: PhantomData<fn() -> M>,
}

impl<'a, M> ModeAccounts<'a, M> {
    fn new(services: &'a Services, client: &'a ClientInfo, provider_id: &'a str, registration: Option<&'a SessionToken>) -> Self {
        let store = services.external_auth.inner.database.external_identities();
        Self { services, client, store, now: Timestamp::now(), provider_id, registration, mode: PhantomData }
    }
}

fn stored(identity: &VerifiedIdentity) -> NewExternalIdentity<'_> {
    NewExternalIdentity {
        provider_id: &identity.provider_id,
        subject: &identity.subject,
        email: identity.email.as_deref(),
        username: identity.username.as_deref(),
    }
}

fn conflict(conflict: LinkConflict) -> completion::LinkConflict {
    match conflict {
        LinkConflict::IdentityTaken => completion::LinkConflict::IdentityTaken,
        LinkConflict::ProviderTaken => completion::LinkConflict::ProviderTaken,
    }
}

type LinkResult = Result<Result<ExternalIdentity, completion::LinkConflict>, ServiceError>;

impl<M: Mode> ExternalAccounts for ModeAccounts<'_, M> {
    type Error = ServiceError;
    type Origin = M::Origin;
    type Linked = ExternalIdentity;
    type Account = M::Account;

    async fn find(&self, provider: &str, subject: &str) -> Result<Option<(ExternalIdentity, M::Origin)>, ServiceError> {
        M::find(&self.store, provider, subject).await
    }
    fn linked_account(&self, linked: &ExternalIdentity) -> AccountId {
        linked.account_id
    }
    async fn touch(&self, linked: &ExternalIdentity, identity: &VerifiedIdentity) -> Result<(), ServiceError> {
        Ok(self.store.touch(linked.id, identity.email.as_deref(), identity.username.as_deref(), self.now).await?)
    }
    async fn active_user(&self, account: AccountId) -> Result<Option<User>, ServiceError> {
        Ok(self.services.external_auth.inner.database.accounts().find_active_user(account).await?)
    }
    async fn user(&self, account: AccountId) -> Result<Option<User>, ServiceError> {
        Ok(self.services.external_auth.inner.database.accounts().find_user(account).await?)
    }
    async fn is_suspended(&self, account: AccountId) -> Result<bool, ServiceError> {
        self.services.accounts.is_suspended(account).await
    }
    async fn link(&self, account: AccountId, identity: &VerifiedIdentity) -> LinkResult {
        Ok(self.store.link(account, stored(identity), self.now).await?.map_err(conflict))
    }
    async fn verified_email_account(&self, email: &str) -> Result<Option<(AccountId, M::Origin)>, ServiceError> {
        M::email(&self.store, email).await
    }
    async fn link_by_email(&self, account: AccountId, identity: &VerifiedIdentity, origin: &M::Origin) -> LinkResult {
        Ok(M::link_accepted(&self.store, account, stored(identity), self.now, origin).await?.map_err(conflict))
    }
    async fn setup_required(&self) -> Result<bool, ServiceError> {
        self.services.accounts.setup_required().await
    }
    async fn registration_open(&self) -> Result<bool, ServiceError> {
        self.services.accounts.registration_open().await
    }
    fn start_registration(&self, identity: VerifiedIdentity, next: String) -> SessionToken {
        self.services.external_auth.start_registration(identity, next)
    }
    async fn finish(&self, account: AccountId, origin: M::Origin) -> Result<M::Account, ServiceError> {
        M::finish(&self.services.external_auth.inner.database, account, origin).await
    }
    fn account_user<'b>(&self, account: &'b M::Account) -> &'b User {
        M::user(account)
    }
    async fn two_factor_enabled(&self, account: AccountId) -> Result<bool, ServiceError> {
        self.services.two_factor.is_enabled(account).await
    }
    fn start_second_factor(&self, account: M::Account, method: &str) -> SessionToken {
        M::pending(&self.services.two_factor, account, method)
    }
    async fn audit(&self, entry: ExternalAudit<'_>) {
        let action = match entry.kind {
            AuditKind::SignIn => AuditAction::SignIn,
            AuditKind::SignInFailed => AuditAction::SignInFailed,
            AuditKind::IdentityLinked => AuditAction::ExternalIdentityLinked,
        };
        let mut record = AuditEntry::new(action);
        if let Some(user) = entry.by {
            record = record.by(user);
        }
        self.services.external_auth.record(record.from_ip(self.client.ip).detail(entry.detail)).await;
    }
}

impl<M: Mode> ExternalRegistrations for ModeAccounts<'_, M> {
    fn site_registration_open(&self) -> bool {
        self.services.instance.current().registration == rbit_core::RegistrationMode::Open
    }
    async fn create(&self, account: NewExternalAccount, identity: &VerifiedIdentity) -> Result<M::Account, CreateFailure<ServiceError>> {
        let NewExternalAccount { name, email, email_verified, registration_open } = account;
        let new_user = NewUser { name, email, display_name: None, password_hash: String::new(), is_admin: false };
        match M::register(&self.store, new_user, email_verified, stored(identity), registration_open).await {
            Ok(Ok(account)) => Ok(account),
            Ok(Err(linked)) => Err(CreateFailure::Linked(conflict(linked))),
            Err(StorageError::Conflict("email")) => Err(CreateFailure::EmailTaken),
            Err(error) => Err(CreateFailure::Store(error.into())),
        }
    }
    fn end_registration(&self) {
        if let Some(token) = self.registration {
            self.services.external_auth.end_registration(token);
        }
    }
    fn created(&self, user: &User) {
        tracing::info!(user = %user.name(), provider = self.provider_id, is_admin = user.is_admin, "account created through a sign-in provider");
        self.services.events.publish(DomainEvent::UserSignedUp { user: user.id() });
    }
}
