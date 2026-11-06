//! Owned plaintext is protected before constructing password-operation futures.

use super::*;

impl Services {
    /// Checks a password sign-in, throttled per client address, per login as typed and
    /// per account it resolves to (so a username and an email share one budget), and
    /// audited. Enrolled users get a pending second step instead of a session.
    ///
    /// A success restores the login's and the account's budget but not the address's:
    /// signing in to one's own account must not buy more guesses at others.
    pub fn sign_in(
        &self,
        login: &str,
        password: String,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<SignInOutcome, ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let (ip, login) = (ip_key(client), login.trim());
            let typed = login_key(login);
            self.throttle.check_client(ThrottleScope::SignIn, &ip)?;
            self.throttle.check(ThrottleScope::SignIn, &typed)?;

            // The account's budget moves in step with the typed login's (checked after it,
            // charged with it), so it only ever runs out first when other aliases spent it.
            let (account, result) = self
                .accounts
                .authenticate_resolved(login, std::mem::take(&mut *password), |account| {
                    account.map_or(Ok(()), |account| self.throttle.check(ThrottleScope::SignIn, &account_key(account)))
                })
                .await;
            let user = match result {
                Ok(user) => user,
                Err(error) => {
                    let reason = match &error {
                        ServiceError::InvalidCredentials => {
                            self.throttle.record_client_failure(ThrottleScope::SignIn, &ip);
                            self.throttle.record_failure(ThrottleScope::SignIn, &typed);
                            if let Some(account) = account {
                                self.throttle.record_failure(ThrottleScope::SignIn, &account_key(account));
                            }
                            "wrong username or password"
                        }
                        ServiceError::Forbidden => "account suspended",
                        _ => return Err(error),
                    };
                    self.audit.record(AuditEntry::new(AuditAction::SignInFailed).by_login(login).from_ip(client.ip).detail(reason)).await;
                    return Err(error);
                }
            };
            self.throttle.reset(ThrottleScope::SignIn, &typed);
            self.throttle.reset(ThrottleScope::SignIn, &account_key(user.id()));

            if self.two_factor.is_enabled(user.id()).await? {
                return Ok(SignInOutcome::SecondFactorRequired(self.two_factor.start_pending(user.id(), "password")));
            }
            self.audit.record(AuditEntry::new(AuditAction::SignIn).by(&user).from_ip(client.ip).detail("password")).await;
            Ok(SignInOutcome::SignedIn(user))
        }
    }

    /// Removes passkey `id`; needs the password and a proof.
    pub fn remove_passkey<'a>(
        &self,
        user: &User,
        id: i64,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let Some(passkey) = self.two_factor.store.passkeys(user.id()).await?.into_iter().find(|passkey| passkey.id == id) else {
                return Err(ServiceError::NotFound);
            };
            self.check_password_and_proof(user, std::mem::take(&mut *password), proof.into(), client).await?;
            self.two_factor.store.remove_passkey(user.id(), id).await?;
            self.audit.record(AuditEntry::new(AuditAction::PasskeyRemoved).by(user).from_ip(client.ip).detail(&passkey.name)).await;
            Ok(())
        }
    }

    /// Removes the authenticator app; needs the password and a proof. Two-factor
    /// authentication stays on if the account has passkeys.
    pub fn disable_two_factor<'a>(
        &self,
        user: &User,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            self.check_password_and_proof(user, std::mem::take(&mut *password), proof.into(), client).await?;
            self.two_factor.store.disable(user.id()).await?;
            self.audit
                .record(AuditEntry::new(AuditAction::TwoFactorDisabled).by(user).from_ip(client.ip).detail("authenticator app"))
                .await;
            Ok(())
        }
    }

    /// Replaces the recovery codes (invalidating the old ones); needs the password and a proof.
    pub fn regenerate_recovery_codes<'a>(
        &self,
        user: &User,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<Vec<String>, ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            self.check_password_and_proof(user, std::mem::take(&mut *password), proof.into(), client).await?;
            let codes = new_recovery_codes();
            if !self.two_factor.store.replace_recovery_codes(user.id(), &digests(user, &codes)).await? {
                return Err(ServiceError::Invalid("two-factor authentication is not enabled".to_owned()));
            }
            self.audit.record(AuditEntry::new(AuditAction::RecoveryCodesRegenerated).by(user).from_ip(client.ip)).await;
            Ok(formatted(&codes))
        }
    }
}
