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
