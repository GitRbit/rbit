//! Lookup, throttle callback, shared admission and covering work have one authority.

use rbit_contracts::AccountId;
use std::future::Future;
use zeroize::Zeroizing;

pub(crate) mod proven;
mod verification;

pub use proven::{OriginalProofReceipt, PasswordProofError, PasswordProofOutcome, ProvenAccount};

use super::{AccountError, AccountRepository};
use crate::{PasswordWork, User};

/// Resolved account identity accompanies success or failure for caller accounting.
pub type AuthenticationOutcome<StoreError, CallbackError> = (Option<AccountId>, Result<User, AccountError<StoreError, CallbackError>>);

#[derive(Debug)]
pub struct PasswordAuthentication<'a, R> {
    repository: &'a R,
    passwords: &'a PasswordWork,
}

impl<'a, R> PasswordAuthentication<'a, R> {
    pub fn new(repository: &'a R, passwords: &'a PasswordWork) -> Self {
        Self { repository, passwords }
    }
}

impl<'a, R: AccountRepository> PasswordAuthentication<'a, R> {
    /// Protects plaintext synchronously, even when the returned future is never polled.
    /// The callback runs after lookup and before admission, including missing users.
    pub fn authenticate_resolved<E: Send + 'a>(
        self,
        login: &'a str,
        password: String,
        before_verify: impl FnOnce(Option<AccountId>) -> Result<(), E> + Send + 'a,
    ) -> impl Future<Output = AuthenticationOutcome<R::Error, E>> + Send + 'a {
        let password = Zeroizing::new(password);
        async move {
            if password.len() > super::registration::MAX_PASSWORD_LEN * 4 {
                return (None, Err(AccountError::InvalidCredentials));
            }
            let record = match self.repository.find_login(login.trim()).await {
                Ok(record) => record,
                Err(error) => return (None, Err(AccountError::Store(error))),
            };
            verification::verify_loaded::<R::Error, E>(self.passwords, password, record, before_verify).await
        }
    }
}
