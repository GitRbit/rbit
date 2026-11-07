//! Captured persistence snapshot; no public constructor/Serde or raw digest Debug.
use super::{Binding, SessionRecord};
use rbit_identity_models::credential::CredentialEpoch;
use std::fmt;

#[derive(Clone)]
pub struct EpochSnapshot {
    pub(super) digest: [u8; 32],
    pub(super) record: SessionRecord,
    pub(super) binding: Option<Binding>,
}
impl EpochSnapshot {
    pub fn record(&self) -> &SessionRecord {
        &self.record
    }
    /// Captured before SQL mutation. This is bounded replay data, not proof that
    /// the session remains live; the final transaction must compare its binding.
    pub fn promotion_spec(&self) -> Option<rbit_identity_models::sessions::SessionPromotion> {
        let binding = self.binding?;
        if self.record.is_expired() {
            return None;
        }
        let mut bytes = [0_u8; 64];
        bytes[..32].copy_from_slice(&self.digest);
        bytes[32..].copy_from_slice(&binding.encode());
        rbit_identity_models::sessions::SessionPromotion::from_storage(self.record.account_id, &bytes)
    }
    /// An absent sidecar is INITIAL only, never a fresh account epoch lookup.
    pub fn epoch(&self) -> CredentialEpoch {
        self.binding.map_or(CredentialEpoch::INITIAL, |binding| binding.epoch)
    }
    pub fn is_legacy_initial(&self) -> bool {
        self.binding.is_none()
    }
}
impl fmt::Debug for EpochSnapshot {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("EpochSnapshot")
            .field("binding", &"<redacted>")
            .field("legacy_initial", &self.is_legacy_initial())
            .finish_non_exhaustive()
    }
}

pub(super) fn same_identity(left: &SessionRecord, right: &SessionRecord) -> bool {
    left.account_id == right.account_id
        && left.created_at == right.created_at
        && left.ip_address == right.ip_address
        && left.user_agent == right.user_agent
}

impl rbit_identity_models::sessions::EpochSessionSnapshot for EpochSnapshot {
    fn digest(&self) -> [u8; 32] {
        self.digest
    }
    fn record(&self) -> &SessionRecord {
        &self.record
    }
    fn epoch(&self) -> CredentialEpoch {
        self.epoch()
    }
    fn is_bound(&self) -> bool {
        self.binding.is_some()
    }
    fn into_stored(self) -> rbit_identity_models::sessions::StoredSession {
        rbit_identity_models::sessions::StoredSession { digest: self.digest, record: self.record }
    }
}
