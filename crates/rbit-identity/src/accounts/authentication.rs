//! Lookup, throttle callback, shared admission and covering work have one authority.

use rbit_contracts::AccountId;
use std::future::Future;
use zeroize::Zeroizing;

use super::{AccountError, AccountRepository};
use crate::{PasswordWork, User};

/// Resolved account identity accompanies success or failure for caller accounting.
pub type AuthenticationOutcome<StoreError, CallbackError> = (Option<AccountId>, Result<User, AccountError<StoreError, CallbackError>>);

#[derive(Debug)]
pub struct PasswordAuthentication<'a, R> {
    repository: &'a R,
    passwords: &'a PasswordWork,
}

impl<'a, R: AccountRepository> PasswordAuthentication<'a, R> {
    pub fn new(repository: &'a R, passwords: &'a PasswordWork) -> Self {
        Self { repository, passwords }
    }

    /// Protects plaintext synchronously, even when the returned future is never polled.
    /// The callback runs after lookup and before admission, including missing users.
    pub fn authenticate_resolved<E: Send + 'a>(
        self,
        login: &'a str,
        password: String,
        before_verify: impl FnOnce(Option<AccountId>) -> Result<(), E> + Send + 'a,
    ) -> impl Future<Output = AuthenticationOutcome<R::Error, E>> + Send + 'a {
        let mut password = Zeroizing::new(password);
        async move {
            if password.len() > super::registration::MAX_PASSWORD_LEN * 4 {
                return (None, Err(AccountError::InvalidCredentials));
            }
            let record = match self.repository.find_login(login.trim()).await {
                Ok(record) => record,
                Err(error) => return (None, Err(AccountError::Store(error))),
            };
            // PHC is protected before any callback/admission early return.
            let (candidate, mut stored) = match record {
                Some(record) => (Some((record.user, record.suspended)), Some(Zeroizing::new(record.password_hash))),
                None => (None, None),
            };
            let account = candidate.as_ref().map(|(user, _)| user.id());
            if let Err(error) = before_verify(account) {
                return (account, Err(AccountError::BeforeVerify(error)));
            }
            let attempt = match self.passwords.begin() {
                Ok(attempt) => attempt,
                Err(error) => return (account, Err(AccountError::Admission(error))),
            };
            let stored = stored.as_mut().map(|stored| std::mem::take(&mut **stored));
            let result = match (attempt.verify_login(std::mem::take(&mut *password), stored).await, candidate) {
                (Ok(true), Some((user, true))) => Err(AccountError::Suspended(user.account.name)),
                (Ok(true), Some((user, false))) => Ok(user),
                (Err(error), _) => Err(AccountError::Password(error)),
                _ => Err(AccountError::InvalidCredentials),
            };
            (account, result)
        }
    }
}
