//! Inactive original-password-proof application; no session issuance authority from User.
use super::super::proof::{ProvenAccount, password_proven};
use super::{AccountError, PasswordAuthentication, verification};
use rbit_contracts::AccountId;
use rbit_identity_models::credential::CredentialLoginRepository;
use std::future::Future;
use zeroize::Zeroizing;

#[derive(Debug)]
pub enum PasswordProofError<Store, Before> {
    Account(AccountError<Store, Before>),
    /// Valid password and active account, but its captured current intent is pending.
    /// Generic temporary infrastructure outcome; never a wrong-password surcharge.
    Pending,
}
pub type PasswordProofOutcome<Store, Before> = (Option<AccountId>, Result<ProvenAccount, PasswordProofError<Store, Before>>);

impl<'a, R: CredentialLoginRepository> PasswordAuthentication<'a, R> {
    /// Explicit inactive seam. Remote issuers must not adopt it until receipt propagation,
    /// fenced readers/writers, persistent legacy interlocks and recovery are enabled together.
    pub fn authenticate_proven_resolved<Before: Send + 'a>(
        self,
        login: &'a str,
        password: String,
        before_verify: impl FnOnce(Option<AccountId>) -> Result<(), Before> + Send + 'a,
    ) -> impl Future<Output = PasswordProofOutcome<R::Error, Before>> + Send + 'a {
        let password = Zeroizing::new(password);
        async move {
            if password.len() > super::super::registration::MAX_PASSWORD_LEN * 4 {
                return (None, Err(PasswordProofError::Account(AccountError::InvalidCredentials)));
            }
            let snapshot = match self.repository.find_login_snapshot(login.trim()).await {
                Ok(snapshot) => snapshot,
                Err(error) => return (None, Err(PasswordProofError::Account(AccountError::Store(error)))),
            };
            let (record, fence) = match snapshot {
                Some(snapshot) => (Some(snapshot.login), Some(snapshot.fence)),
                None => (None, None),
            };
            let (account, result) = verification::verify_loaded::<R::Error, Before>(self.passwords, password, record, before_verify).await;
            let result = match (result, fence) {
                (Err(error), _) => Err(PasswordProofError::Account(error)),
                (Ok(_), Some(fence)) if fence.pending => Err(PasswordProofError::Pending),
                (Ok(user), Some(fence)) => Ok(password_proven(user, fence.epoch)),
                (Ok(_), None) => Err(PasswordProofError::Account(AccountError::InvalidCredentials)),
            };
            (account, result)
        }
    }
}
