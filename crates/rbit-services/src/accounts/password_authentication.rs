//! Credential API compatibility over the identity-owned decision workflow.

use rbit_core::{AccountId, User};
use rbit_identity::accounts::PasswordAuthentication;

use super::{AccountService, application_error};
use crate::ServiceError;

impl AccountService {
    /// Verifies through the shared owner; suspension is disclosed only after success.
    pub fn authenticate<'a>(&'a self, login: &'a str, password: String) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
        let operation = self.authenticate_resolved(login, password, |_| Ok(()));
        async move { operation.await.1 }
    }

    /// Account-scoped read-only throttling keeps its original pre-admission precedence.
    pub(crate) fn authenticate_resolved<'a>(
        &'a self,
        login: &'a str,
        password: String,
        before_verify: impl FnOnce(Option<AccountId>) -> Result<(), ServiceError> + Send + 'a,
    ) -> impl Future<Output = (Option<AccountId>, Result<User, ServiceError>)> + Send + 'a {
        let operation = PasswordAuthentication::new(&self.accounts, &self.passwords).authenticate_resolved(login, password, before_verify);
        async move {
            let (account, result) = operation.await;
            (account, result.map_err(application_error::authentication))
        }
    }
}
