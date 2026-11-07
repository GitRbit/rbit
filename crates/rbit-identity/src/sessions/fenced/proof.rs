//! Only identity-owned verified original origins can select the fenced issuer.
#[cfg(feature = "password-work")]
use crate::accounts::proof::OriginalProofReceipt;
use rbit_contracts::AccountId;
use rbit_identity_models::credential::CredentialEpoch;
mod sealed {
    pub trait Sealed {}
}
/// Sealed authentication evidence, never constructible from User or a fresh epoch.
///
/// ```compile_fail
/// use rbit_identity::sessions::EpochIssuanceProof;
/// struct UserClaim;
/// impl EpochIssuanceProof for UserClaim {
///     fn account(&self) -> rbit_identity::AccountId { rbit_identity::AccountId(1) }
///     fn original_epoch(&self) -> rbit_identity_models::credential::CredentialEpoch {
///         rbit_identity_models::credential::CredentialEpoch::INITIAL
///     }
/// }
/// ```
pub trait EpochIssuanceProof: sealed::Sealed + Sync {
    fn account(&self) -> AccountId;
    fn original_epoch(&self) -> CredentialEpoch;
}
#[cfg(feature = "password-work")]
impl sealed::Sealed for OriginalProofReceipt {}
#[cfg(feature = "password-work")]
impl EpochIssuanceProof for OriginalProofReceipt {
    fn account(&self) -> AccountId {
        self.account()
    }
    fn original_epoch(&self) -> CredentialEpoch {
        self.original_epoch()
    }
}
