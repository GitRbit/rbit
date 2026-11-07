//! Shared verification order for legacy and original-proof applications.
use super::{AccountError, AuthenticationOutcome};
use crate::{PasswordWork, accounts::LoginRecord};
use rbit_contracts::AccountId;
use std::future::Future;
use zeroize::Zeroizing;

pub(super) fn verify_loaded<'a, Store: Send + 'a, Before: Send + 'a>(
    passwords: &'a PasswordWork,
    mut password: Zeroizing<String>,
    record: Option<LoginRecord>,
    before_verify: impl FnOnce(Option<AccountId>) -> Result<(), Before> + Send + 'a,
) -> impl Future<Output = AuthenticationOutcome<Store, Before>> + Send + 'a {
    // Protect PHC before constructing even this unpolled inner future.
    let (candidate, mut stored) = match record {
        Some(record) => (Some((record.user, record.suspended)), Some(Zeroizing::new(record.password_hash))),
        None => (None, None),
    };
    let account = candidate.as_ref().map(|(user, _)| user.id());
    async move {
        if let Err(error) = before_verify(account) {
            return (account, Err(AccountError::BeforeVerify(error)));
        }
        let attempt = match passwords.begin() {
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
