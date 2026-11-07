//! Bounded durable promotion data; never authentication/session-issuance receipts.
use crate::{TokenDigest, credential::CredentialEpoch};
use rbit_contracts::AccountId;

/// Digest + original account/epoch/incarnation, captured before a credential write.
/// Reconstruction is only checked persistence data, not evidence it is live/current.
/// No Serde/Debug byte exposure; the backend must match the actual record transaction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SessionPromotion {
    bytes: [u8; 64],
    account: AccountId,
    original: CredentialEpoch,
}
impl SessionPromotion {
    pub fn from_storage(account: AccountId, bytes: &[u8]) -> Option<Self> {
        let bytes = <[u8; 64]>::try_from(bytes).ok()?;
        let stored = i64::from_be_bytes(bytes[32..40].try_into().ok()?);
        let original = CredentialEpoch::new(i64::from_be_bytes(bytes[40..48].try_into().ok()?))?;
        if stored != account.0 || original == CredentialEpoch::INITIAL {
            return None;
        }
        Some(Self { bytes, account, original })
    }
    pub fn account(self) -> AccountId {
        self.account
    }
    pub fn original_epoch(self) -> CredentialEpoch {
        self.original
    }
    pub fn digest(self) -> TokenDigest {
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&self.bytes[..32]);
        digest
    }
    pub fn storage_bytes(self) -> [u8; 64] {
        self.bytes
    }
}
impl std::fmt::Debug for SessionPromotion {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SessionPromotion").field("binding", &"<redacted>").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRevocationDecision {
    EndAll,
    KeepCurrent(SessionPromotion),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRevocationOutcome {
    EndedAll,
    KeptCurrent,
}

/// Fixed durable request supplied by trusted persistence composition, not a proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionRevocationRequest {
    id: i64,
    account: AccountId,
    target: CredentialEpoch,
    decision: SessionRevocationDecision,
}
impl SessionRevocationRequest {
    pub fn from_storage(id: i64, account: AccountId, target: CredentialEpoch, decision: SessionRevocationDecision) -> Option<Self> {
        if id <= 0 || target == CredentialEpoch::INITIAL {
            return None;
        }
        if let SessionRevocationDecision::KeepCurrent(spec) = decision
            && (spec.account() != account || spec.original_epoch() >= target)
        {
            return None;
        }
        Some(Self { id, account, target, decision })
    }
    pub fn id(self) -> i64 {
        self.id
    }
    pub fn account(self) -> AccountId {
        self.account
    }
    pub fn target(self) -> CredentialEpoch {
        self.target
    }
    pub fn decision(self) -> SessionRevocationDecision {
        self.decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes(account: i64, epoch: i64) -> [u8; 64] {
        let mut bytes = [b'X'; 64];
        bytes[32..40].copy_from_slice(&account.to_be_bytes());
        bytes[40..48].copy_from_slice(&epoch.to_be_bytes());
        bytes
    }
    #[test]
    fn bounded_promotion_and_requests_preserve_exact_checked_origin() {
        let value = bytes(7, 1);
        let spec = SessionPromotion::from_storage(AccountId(7), &value).unwrap();
        assert_eq!(spec.storage_bytes(), value);
        assert_eq!(spec.digest(), [b'X'; 32]);
        assert_eq!(spec.original_epoch().get(), 1);
        for bad in [&value[..63], &[][..]] {
            assert!(SessionPromotion::from_storage(AccountId(7), bad).is_none());
        }
        assert!(SessionPromotion::from_storage(AccountId(8), &value).is_none());
        for epoch in [0, -1, i64::MIN] {
            assert!(SessionPromotion::from_storage(AccountId(7), &bytes(7, epoch)).is_none());
        }
        assert!(
            SessionRevocationRequest::from_storage(
                1,
                AccountId(7),
                CredentialEpoch::new(2).unwrap(),
                SessionRevocationDecision::KeepCurrent(spec)
            )
            .is_some()
        );
        assert!(
            SessionRevocationRequest::from_storage(
                1,
                AccountId(7),
                CredentialEpoch::new(1).unwrap(),
                SessionRevocationDecision::KeepCurrent(spec)
            )
            .is_none()
        );
        assert!(
            SessionRevocationRequest::from_storage(
                1,
                AccountId(8),
                CredentialEpoch::new(2).unwrap(),
                SessionRevocationDecision::KeepCurrent(spec)
            )
            .is_none()
        );
        assert!(
            SessionRevocationRequest::from_storage(0, AccountId(7), CredentialEpoch::new(2).unwrap(), SessionRevocationDecision::EndAll)
                .is_none()
        );
        assert!(
            SessionRevocationRequest::from_storage(1, AccountId(7), CredentialEpoch::INITIAL, SessionRevocationDecision::EndAll).is_none()
        );
        let debug = format!("{spec:?}");
        assert!(debug.contains("redacted"));
        assert!(!debug.contains("XXXX"));
    }
    #[test]
    fn maximum_epoch_never_wraps_into_a_valid_promotion_target() {
        let spec = SessionPromotion::from_storage(AccountId(7), &bytes(7, i64::MAX)).unwrap();
        assert!(
            SessionRevocationRequest::from_storage(
                1,
                AccountId(7),
                CredentialEpoch::new(i64::MAX).unwrap(),
                SessionRevocationDecision::KeepCurrent(spec)
            )
            .is_none()
        );
        let spec = SessionPromotion::from_storage(AccountId(7), &bytes(7, i64::MAX - 1)).unwrap();
        assert!(
            SessionRevocationRequest::from_storage(
                1,
                AccountId(7),
                CredentialEpoch::new(i64::MAX).unwrap(),
                SessionRevocationDecision::KeepCurrent(spec)
            )
            .is_some()
        );
    }
}
