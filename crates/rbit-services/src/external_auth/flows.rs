//! Existing callback policy is shared by legacy and inactive fixed-origin producers.
use super::original::{Completion, Legacy, Mode, Original, OriginalExternalSignIn};
use super::{
    AccountName, AuditAction, AuditEntry, Callback, ClientInfo, DomainEvent, Email, ExternalAuthService, ExternalIdentity, ExternalSignIn,
    LinkConflict, NewExternalIdentity, NewUser, ProviderConfig, ServiceError, Services, SessionToken, StorageError, ThrottleScope,
    Timestamp, User, VerifiedIdentity, ip_key, linked_entry,
};

impl Services {
    /// Decides what a verified callback means — sign in, second step, link, register or
    /// refuse — following the rules in the module documentation.
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
        let external = &self.external_auth;
        let Callback { identity, next, link_account } = callback;
        let config = external.provider(&identity.provider_id).ok_or(ServiceError::NotFound)?.config.clone();
        let store = external.inner.database.external_identities();
        let now = Timestamp::now();
        let new = NewExternalIdentity {
            provider_id: &identity.provider_id,
            subject: &identity.subject,
            email: identity.email.as_deref(),
            username: identity.username.as_deref(),
        };
        let taken = || ServiceError::Invalid(format!("this {} account is already linked to another account here", config.name));

        if let Some((linked, origin)) = M::find(&store, &identity.provider_id, &identity.subject).await? {
            if let Some(account) = link_account {
                return if linked.account_id == account { Ok(Completion::Linked) } else { Err(taken()) };
            }
            store.touch(linked.id, new.email, new.username, now).await?;
            return self.finish_external_with::<M>(&linked, origin, &identity, &config, client).await;
        }

        if let Some(account) = link_account {
            let user = external.inner.database.accounts().find_active_user(account).await?.ok_or(ServiceError::Forbidden)?;
            return match store.link(account, new, now).await? {
                Ok(_) => {
                    external.record(linked_entry(&user, &config, &identity, client, "")).await;
                    Ok(Completion::Linked)
                }
                Err(LinkConflict::IdentityTaken) => Err(taken()),
                Err(LinkConflict::ProviderTaken) => Err(ServiceError::Invalid(format!(
                    "your account is already linked to another {} account; unlink it first",
                    config.name
                ))),
            };
        }

        if config.trust_email
            && identity.email_verified
            && let Some(email) = identity.email.as_deref()
            && let Some((account, origin)) = M::email(&store, email).await?
        {
            match store.link(account, new, now).await? {
                Ok(linked) => {
                    if let Some(user) = external.inner.database.accounts().find_active_user(account).await? {
                        external.record(linked_entry(&user, &config, &identity, client, ", by verified email")).await;
                    }
                    return self.finish_external_with::<M>(&linked, origin, &identity, &config, client).await;
                }
                Err(LinkConflict::ProviderTaken) => {
                    return Err(ServiceError::Invalid(format!(
                        "the account with this email address is linked to a different {} account",
                        config.name
                    )));
                }
                Err(LinkConflict::IdentityTaken) => return Err(taken()),
            }
        }

        if self.external_registration_allowed(&config).await? {
            return Ok(Completion::Register(external.start_registration(identity, next)));
        }
        let entry = AuditEntry::new(AuditAction::SignInFailed).from_ip(client.ip).detail(format!("{}: no linked account", config.name));
        external.record(entry).await;
        Err(ServiceError::Invalid(format!(
            "no account here is linked to this {name} account, and new accounts can't be created with it. If you have an account, \
             sign in with your password and link {name} under Settings → Security",
            name = config.name
        )))
    }

    /// Whether a first sign-in with the provider may create an account now. Never on an
    /// instance without users: its first account becomes the administrator, which takes
    /// the setup token (see [`crate::AccountService::sign_up`]).
    async fn external_registration_allowed(&self, config: &ProviderConfig) -> Result<bool, ServiceError> {
        if self.accounts.setup_required().await? {
            return Ok(false);
        }
        Ok(ExternalAuthService::may_register(config, self.accounts.registration_open().await?))
    }

    /// Signs in as the account `linked` belongs to: refused while suspended, continued at
    /// the second step when the account has 2FA (unless the provider's MFA is trusted).
    async fn finish_external_with<M: Mode>(
        &self,
        linked: &ExternalIdentity,
        origin: M::Origin,
        identity: &VerifiedIdentity,
        config: &ProviderConfig,
        client: &ClientInfo,
    ) -> Result<Completion<M::Account>, ServiceError> {
        let external = &self.external_auth;
        if self.accounts.is_suspended(linked.account_id).await? {
            let mut entry =
                AuditEntry::new(AuditAction::SignInFailed).from_ip(client.ip).detail(format!("{}: account suspended", config.name));
            if let Some(user) = external.inner.database.accounts().find_user(linked.account_id).await? {
                entry = entry.by(&user);
            }
            external.record(entry).await;
            return Err(ServiceError::Forbidden);
        }
        let account = M::finish(&external.inner.database, linked.account_id, origin).await?;
        let user = M::user(&account);
        let trusted_mfa = config.trust_mfa && identity.mfa;
        if !trusted_mfa && self.two_factor.is_enabled(user.id()).await? {
            return Ok(Completion::SecondFactorRequired(M::pending(&self.two_factor, account, &config.name)));
        }
        let detail = if trusted_mfa { format!("{} (provider MFA)", config.name) } else { config.name.clone() };
        external.record(AuditEntry::new(AuditAction::SignIn).by(user).from_ip(client.ip).detail(detail)).await;
        Ok(Completion::SignedIn(account))
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
        if !self.external_registration_allowed(&config).await? {
            return Err(ServiceError::RegistrationClosed);
        }
        let name: AccountName = username.trim().parse()?;
        let email: Email = email.trim().parse()?;
        let identity = &pending.identity;
        let verified = identity.email_verified && identity.email.as_deref().is_some_and(|given| given.eq_ignore_ascii_case(email.as_str()));
        let registration_open =
            ExternalAuthService::may_register(&config, self.instance.current().registration == rbit_core::RegistrationMode::Open);
        let new_user = NewUser { name, email, display_name: None, password_hash: String::new(), is_admin: false };
        let new = NewExternalIdentity {
            provider_id: &identity.provider_id,
            subject: &identity.subject,
            email: identity.email.as_deref(),
            username: identity.username.as_deref(),
        };
        let account = match M::register(&external.inner.database.external_identities(), new_user, verified, new, registration_open).await {
            Ok(Ok(user)) => user,
            Ok(Err(_)) => {
                external.end_registration(token);
                return Err(ServiceError::Invalid(format!(
                    "this {} account is already linked to an account here; sign in instead",
                    config.name
                )));
            }
            Err(StorageError::Conflict("email")) => {
                return Err(ServiceError::Invalid(format!(
                    "an account already uses this email address; sign in to it and link {} under Settings → Security",
                    config.name
                )));
            }
            Err(error) => return Err(error.into()),
        };
        external.end_registration(token);
        let user = M::user(&account);
        tracing::info!(user = %user.name(), provider = provider_id, is_admin = user.is_admin, "account created through a sign-in provider");
        self.events.publish(DomainEvent::UserSignedUp { user: user.id() });
        external.record(linked_entry(user, &config, identity, client, ", new account")).await;
        external.record(AuditEntry::new(AuditAction::SignIn).by(user).from_ip(client.ip).detail(config.name.clone())).await;
        Ok((account, pending.next))
    }
}
