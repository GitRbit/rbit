//! Credential checking uses identical admission and covering work for every login shape.

use rbit_core::{AccountId, User};

use super::{AccountService, MAX_PASSWORD_LEN};
use crate::ServiceError;

impl AccountService {
    /// Verifies credentials with shared admission and dummy work for missing or invalid stored hashes.
    /// A suspended user with the right password gets [`ServiceError::Forbidden`].
    pub async fn authenticate(&self, login: &str, password: String) -> Result<User, ServiceError> {
        self.authenticate_resolved(login, password, |_| Ok(())).await.1
    }

    /// Like [`Self::authenticate`], also returning the account `login` resolved to (if
    /// any), so callers can throttle per account. `before_verify` sees that account before
    /// the password is checked; an error from it is returned instead of checking.
    pub(crate) async fn authenticate_resolved(
        &self,
        login: &str,
        password: String,
        before_verify: impl FnOnce(Option<AccountId>) -> Result<(), ServiceError>,
    ) -> (Option<AccountId>, Result<User, ServiceError>) {
        if password.len() > MAX_PASSWORD_LEN * 4 {
            return (None, Err(ServiceError::InvalidCredentials));
        }
        let mut record = match self.accounts.find_login(login.trim()).await {
            Ok(record) => record,
            Err(error) => return (None, Err(error.into())),
        };
        let account = record.as_ref().map(|record| record.user.id());
        if let Err(error) = before_verify(account) {
            return (account, Err(error));
        }
        let attempt = match self.passwords.begin() {
            Ok(attempt) => attempt,
            Err(error) => return (account, Err(error.into())),
        };
        let stored = record.as_mut().map(|record| std::mem::take(&mut record.password_hash));
        let result = match (attempt.verify_login(password, stored).await, record) {
            (Ok(true), Some(record)) if record.suspended => {
                tracing::info!(user = %record.user.name(), "suspended user refused sign-in");
                Err(ServiceError::Forbidden)
            }
            (Ok(true), Some(record)) => Ok(record.user),
            (Err(error), _) => Err(error.into()),
            _ => Err(ServiceError::InvalidCredentials),
        };
        (account, result)
    }
}
