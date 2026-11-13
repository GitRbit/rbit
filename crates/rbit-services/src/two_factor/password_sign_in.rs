//! One statically selected password sign-in/accounting workflow for both proof modes.
use super::{
    AuditAction, AuditEntry, ClientInfo, ServiceError, Services, SessionToken, SignInOutcome, ThrottleScope, User, account_key, ip_key,
    login_key,
};
use crate::accounts::ProvenAccount;

/// Inactive original-proof result. A signed-in account still needs the eventual
/// fenced session issuer; it must not be passed to the legacy User-only issuer.
#[derive(Debug)]
pub enum OriginalSignInOutcome {
    SignedIn(ProvenAccount),
    SecondFactorRequired(SessionToken),
}

enum PasswordSignInAccount {
    Legacy(User),
    Original(ProvenAccount),
}
impl PasswordSignInAccount {
    fn user(&self) -> &User {
        match self {
            Self::Legacy(user) => user,
            Self::Original(proven) => proven.user(),
        }
    }
}
enum PasswordSignInCompletion {
    SignedIn(PasswordSignInAccount),
    SecondFactorRequired(SessionToken),
}

impl Services {
    /// Checks a password sign-in, throttled per client address, typed login and
    /// resolved account, and audited. Enrolled users get a pending second step.
    /// Success restores login/account budgets but never the address's budget.
    pub fn sign_in(
        &self,
        login: &str,
        password: String,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<SignInOutcome, ServiceError>> {
        let operation = self.password_sign_in::<false>(login, password, client);
        async move {
            match operation.await? {
                PasswordSignInCompletion::SignedIn(PasswordSignInAccount::Legacy(user)) => Ok(SignInOutcome::SignedIn(user)),
                PasswordSignInCompletion::SecondFactorRequired(pending) => Ok(SignInOutcome::SecondFactorRequired(pending)),
                PasswordSignInCompletion::SignedIn(PasswordSignInAccount::Original(_)) => Err(ServiceError::NotFound),
            }
        }
    }

    /// Inactive closed first-factor workflow: preserves the SAME pre-verification
    /// epoch through optional local MFA. No route selects this until the complete
    /// writer/issuer/reader/recovery protocol is activated together.
    pub fn sign_in_original(
        &self,
        login: &str,
        password: String,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<OriginalSignInOutcome, ServiceError>> {
        let operation = self.password_sign_in::<true>(login, password, client);
        async move {
            match operation.await? {
                PasswordSignInCompletion::SignedIn(PasswordSignInAccount::Original(proven)) => Ok(OriginalSignInOutcome::SignedIn(proven)),
                PasswordSignInCompletion::SecondFactorRequired(pending) => Ok(OriginalSignInOutcome::SecondFactorRequired(pending)),
                PasswordSignInCompletion::SignedIn(PasswordSignInAccount::Legacy(_)) => Err(ServiceError::NotFound),
            }
        }
    }

    // Private const mode is sealed to the two wrappers above. Both use the same
    // verification owners, decision order and accounting; no dynamic dispatch,
    // boxed future, password clone, or extra legacy lookup/crypto is introduced.
    fn password_sign_in<const ORIGINAL: bool>(
        &self,
        login: &str,
        password: String,
        client: &ClientInfo,
    ) -> impl std::future::Future<Output = Result<PasswordSignInCompletion, ServiceError>> {
        let mut password = zeroize::Zeroizing::new(password);
        async move {
            let (ip, login) = (ip_key(client), login.trim());
            // Per login and per account, the budgets are kept per client network: wrong
            // guesses from one network spend its own, so a stranger cannot keep the owner
            // out of their account. An account-wide budget, five times larger, still bounds
            // guesses spread over many networks.
            // Keep the network before the potentially long email: throttle keys
            // are length-capped, so appending it could merge different networks.
            let typed = format!("{ip}@{}", login_key(login));
            let local = |account: rbit_core::AccountId| format!("{}@{ip}", account_key(account));
            self.throttle.check_client(ThrottleScope::SignIn, &ip)?;
            self.throttle.check(ThrottleScope::SignIn, &typed)?;
            let before_verify = |account: Option<rbit_core::AccountId>| {
                account.map_or(Ok(()), |account| {
                    self.throttle.check(ThrottleScope::SignIn, &local(account))?;
                    self.throttle.check_client_failures(ThrottleScope::SignIn, &account_key(account))
                })
            };
            let (account, result) = if ORIGINAL {
                let (account, result) =
                    self.accounts.authenticate_original_proof(login, std::mem::take(&mut *password), before_verify).await;
                (account, result.map(PasswordSignInAccount::Original))
            } else {
                let (account, result) = self.accounts.authenticate_resolved(login, std::mem::take(&mut *password), before_verify).await;
                (account, result.map(PasswordSignInAccount::Legacy))
            };
            let account = match result {
                Ok(account) => account,
                Err(error) => {
                    let reason = match &error {
                        ServiceError::InvalidCredentials => {
                            self.throttle.record_client_failure(ThrottleScope::SignIn, &ip);
                            self.throttle.record_failure(ThrottleScope::SignIn, &typed);
                            if let Some(account) = account {
                                self.throttle.record_failure(ThrottleScope::SignIn, &local(account));
                                self.throttle.record_client_failure(ThrottleScope::SignIn, &account_key(account));
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
            let user = account.user();
            self.throttle.reset(ThrottleScope::SignIn, &typed);
            self.throttle.reset(ThrottleScope::SignIn, &local(user.id()));
            if self.two_factor.is_enabled(user.id()).await? {
                let pending = match account {
                    PasswordSignInAccount::Legacy(user) => self.two_factor.start_pending(user.id(), "password"),
                    PasswordSignInAccount::Original(proven) => self.two_factor.start_original_password_pending(proven),
                };
                return Ok(PasswordSignInCompletion::SecondFactorRequired(pending));
            }
            self.audit.record(AuditEntry::new(AuditAction::SignIn).by(user).from_ip(client.ip).detail("password")).await;
            Ok(PasswordSignInCompletion::SignedIn(account))
        }
    }
}
