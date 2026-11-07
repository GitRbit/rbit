//! Opaque producer evidence; constructors stay private to feature applications.
use crate::User;
use rbit_contracts::AccountId;
use rbit_identity_models::{accounts::CreatedAccountSnapshot, credential::CredentialEpoch};
use std::fmt;

/// Fixed original facts, minted only by verified-password/provider or committed-creation applications.
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

pub(super) fn password_proven(user: User, original: CredentialEpoch) -> ProvenAccount {
    let receipt = OriginalProofReceipt { account: user.id(), original };
    ProvenAccount { user, receipt }
}
pub(super) fn creation_proven(created: CreatedAccountSnapshot) -> ProvenAccount {
    let receipt = OriginalProofReceipt { account: created.user.id(), original: created.epoch };
    ProvenAccount { user: created.user, receipt }
}
pub(super) fn provider_proven(user: User, original: CredentialEpoch) -> ProvenAccount {
    let receipt = OriginalProofReceipt { account: user.id(), original };
    ProvenAccount { user, receipt }
}
