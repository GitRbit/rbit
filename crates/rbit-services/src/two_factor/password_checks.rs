//! Admission precedes one-use proof consumption; capacity failures are not bad credentials.

use super::verification::Claims;
use super::{GitPasswordOutcome, SecondFactorProof, Throttle, ip_key};
use crate::{ServiceError, Services, ThrottleScope};
use rbit_core::User;
use rbit_identity_models::sessions::ClientInfo;

impl Services {
    /// Re-checks the password and a proof before a sensitive change (throttled and
    /// audited per account); the identity step decides the order.
    pub(super) fn check_password_and_proof(
        &self,
        user: &User,
        password: String,
        proof: SecondFactorProof<'_>,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<(), ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let throttle = Throttle(&self.throttle);
            let claims = Claims::legacy(&self.two_factor.store);
            let step = self.two_factor.second_step(&throttle);
            match step.confirm(user, std::mem::take(&mut *password), proof, &claims).await {
                Ok(()) => Ok(()),
                Err(error) => Err(self.settings_failure(error, client).await),
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
