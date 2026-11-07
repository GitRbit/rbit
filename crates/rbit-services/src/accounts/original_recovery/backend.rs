//! A global owner lease spans classify→apply→ack, never Argon2, network or audit.
use super::EpochSessionRecovery;
use rbit_core::Timestamp;
use rbit_identity_models::sessions::{
    CompletedSessionRevocation, PendingSessionRevocation, SessionPairIdentity, SessionRevocationAcknowledgment,
    SessionRevocationRepository, SessionRevocationRequest, SessionRevocationState,
};
use rbit_storage::{AppliedSessionRevocation, CredentialRecoveryPermit, StorageError};
use std::num::NonZeroU8;

pub(super) struct RecoveryLease<'a> {
    owner: &'a EpochSessionRecovery,
    _serial: CredentialRecoveryPermit,
    pair: SessionPairIdentity,
}
impl<'a> RecoveryLease<'a> {
    pub async fn new(owner: &'a EpochSessionRecovery, serial: CredentialRecoveryPermit) -> Result<Self, StorageError> {
        let pair = owner.revocations.verify_pair(&owner.sessions).await?;
        Ok(Self { owner, _serial: serial, pair })
    }
}
impl SessionRevocationRepository for RecoveryLease<'_> {
    type Error = StorageError;
    type Applied = AppliedSessionRevocation;
    fn pending(&self, after: i64, limit: NonZeroU8) -> impl Future<Output = Result<Vec<PendingSessionRevocation>, Self::Error>> + Send {
        self.owner.revocations.pending(after, limit)
    }
    fn state(&self, pending: &PendingSessionRevocation) -> impl Future<Output = Result<SessionRevocationState, Self::Error>> + Send {
        self.owner.revocations.state(pending)
    }
    fn apply(&self, request: SessionRevocationRequest) -> impl Future<Output = Result<Self::Applied, Self::Error>> + Send {
        self.owner.sessions.apply_pending_revocation(request, self.pair)
    }
    fn acknowledge(
        &self,
        request: &SessionRevocationRequest,
        applied: &Self::Applied,
    ) -> impl Future<Output = Result<SessionRevocationAcknowledgment, Self::Error>> + Send {
        self.owner.revocations.acknowledge(request, &self.owner.sessions, applied, Timestamp::now())
    }
    fn supersede(
        &self,
        pending: &PendingSessionRevocation,
        witness: &CompletedSessionRevocation,
    ) -> impl Future<Output = Result<SessionRevocationAcknowledgment, Self::Error>> + Send {
        self.owner.revocations.supersede(pending, witness)
    }
}
