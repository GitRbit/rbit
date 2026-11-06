//! Inactive original-password-proof application; no session issuance authority from User.
use super::{AccountError, PasswordAuthentication, verification};
use crate::User;
use rbit_contracts::AccountId;
use rbit_identity_models::credential::{CredentialEpoch, CredentialLoginRepository};
use std::{fmt, future::Future};
use zeroize::Zeroizing;

/// Fixed original pre-verification facts, minted only after successful password work.
/// No public constructor or Serde implementation. Holding User/current epoch is not proof.
///
/// ```compile_fail
/// use rbit_identity::accounts::OriginalProofReceipt;
/// use rbit_identity::AccountId;
/// use rbit_identity_models::credential::CredentialEpoch;
/// let _ = OriginalProofReceipt { account: AccountId(1), original: CredentialEpoch::INITIAL };
/// ```
///
/// ```compile_fail
/// use rbit_identity::accounts::OriginalProofReceipt;
/// fn serialize(receipt: &OriginalProofReceipt) { serde_json::to_string(receipt).unwrap(); }
/// ```
#[derive(Clone)]
pub struct OriginalProofReceipt {
    account: AccountId,
    original: CredentialEpoch,
}
impl OriginalProofReceipt {
    pub fn account(&self) -> AccountId {
        self.account
    }
    pub fn original_epoch(&self) -> CredentialEpoch {
        self.original
    }
    /// Reattach authoritative user data without changing the original proof's epoch.
    /// The future MFA/provider adapter must obtain this user through its fenced lookup.
    pub fn bind_user(self, user: User) -> Result<ProvenAccount, Self> {
        if user.id() != self.account {
            return Err(self);
        }
        Ok(ProvenAccount { user, receipt: self })
    }
}
impl fmt::Debug for OriginalProofReceipt {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("OriginalProofReceipt").field("proof", &"<redacted>").finish_non_exhaustive()
    }
}

/// Successfully verified account plus its unrefreshed original proof facts.
pub struct ProvenAccount {
    user: User,
    receipt: OriginalProofReceipt,
}
impl ProvenAccount {
    pub fn user(&self) -> &User {
        &self.user
    }
    pub fn receipt(&self) -> &OriginalProofReceipt {
        &self.receipt
    }
    pub fn into_parts(self) -> (User, OriginalProofReceipt) {
        (self.user, self.receipt)
    }
}
impl fmt::Debug for ProvenAccount {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("ProvenAccount").field("proof", &"<redacted>").finish_non_exhaustive()
    }
}

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
                (Ok(user), Some(fence)) => {
                    let receipt = OriginalProofReceipt { account: user.id(), original: fence.epoch };
                    Ok(ProvenAccount { user, receipt })
                }
                (Ok(_), None) => Err(PasswordProofError::Account(AccountError::InvalidCredentials)),
            };
            (account, result)
        }
    }
}
