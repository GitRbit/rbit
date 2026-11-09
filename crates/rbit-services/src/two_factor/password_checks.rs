//! Admission precedes one-use proof consumption; capacity failures are not bad credentials.

use super::{Ceremony, GitPasswordOutcome, SecondFactorProof, TwoFactorService, ip_key};
use crate::audit::{AuditAction, AuditEntry};
use crate::{ServiceError, Services, ThrottleScope};
use rbit_core::User;
use rbit_identity::PasswordAttempt;
use rbit_identity_models::sessions::ClientInfo;

impl TwoFactorService {
    /// Re-checks the account password for sensitive changes.
    fn verify_password(
        &self,
        user: &User,
        password: String,
        attempt: PasswordAttempt,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let Some(record) = self.accounts.find_login_by_id(user.id()).await? else {
                return Err(ServiceError::InvalidCredentials);
            };
            if attempt.verify(std::mem::take(&mut *password), record.password_hash).await? {
                Ok(())
            } else {
                Err(ServiceError::InvalidCredentials)
            }
        }
    }
}

impl Services {
    pub(super) fn check_password_and_proof(
        &self,
        user: &User,
        password: String,
        proof: SecondFactorProof<'_>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let key = user.id().0.to_string();
            self.throttle.check(ThrottleScope::TwoFactor, &key)?;
            let attempt = self.two_factor.passwords.begin()?;
            // Taken before anything is checked, so a failed attempt can't be retried with it.
            let challenge = match proof {
                SecondFactorProof::Passkey(response) => self.two_factor.take_challenge(user.id(), Ceremony::Confirm, response),
                SecondFactorProof::Code(_) => None,
            };
            let verified = match self.two_factor.verify_password(user, std::mem::take(&mut *password), attempt).await {
                Ok(()) => self.two_factor.verify(user, proof, challenge).await.map(drop),
                Err(error) => Err(error),
            };
            match verified {
                Ok(()) => {
                    self.throttle.reset(ThrottleScope::TwoFactor, &key);
                    Ok(())
                }
                Err(ServiceError::InvalidCredentials) => {
                    self.throttle.record_failure(ThrottleScope::TwoFactor, &key);
                    self.audit
                        .record(AuditEntry::new(AuditAction::TwoFactorFailed).by(user).from_ip(client.ip).detail("in settings"))
                        .await;
                    Err(ServiceError::InvalidCredentials)
                }
                Err(other) => Err(other),
            }
        }
    }

    /// Checks Git HTTP Basic credentials made of a login and the account password,
    /// throttled per client address and per login. Accounts with 2FA must use tokens.
    pub fn authenticate_git_password(
        &self,
        login: &str,
        password: String,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<GitPasswordOutcome, ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let (ip, login) = (ip_key(client), login.trim());
            self.throttle.check_client(ThrottleScope::GitAuth, &ip)?;
            self.throttle.check(ThrottleScope::GitAuth, login)?;
            let user = match self.accounts.authenticate(login, std::mem::take(&mut *password)).await {
                Ok(user) => user,
                Err(error) => {
                    if matches!(error, ServiceError::InvalidCredentials) {
                        self.throttle.record_client_failure(ThrottleScope::GitAuth, &ip);
                        self.throttle.record_failure(ThrottleScope::GitAuth, login);
                    }
                    return Err(error);
                }
            };
            self.throttle.reset(ThrottleScope::GitAuth, login);
            if self.accounts.password_change_required(user.id()).await? {
                return Err(ServiceError::Forbidden);
            }
            if self.two_factor.is_enabled(user.id()).await? {
                return Ok(GitPasswordOutcome::TokenRequired);
            }
            Ok(GitPasswordOutcome::Authenticated(user))
        }
    }
}
