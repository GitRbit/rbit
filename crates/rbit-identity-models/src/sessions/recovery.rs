//! Checked durable recovery facts and a narrow static port; no runtime or crypto.
use super::{SessionRevocationDecision, SessionRevocationRequest};
use crate::credential::CredentialEpoch;
use rbit_contracts::AccountId;
use std::{future::Future, num::NonZeroU8};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingSessionRevocation {
    id: i64,
    account: AccountId,
    target: CredentialEpoch,
    decision: Option<SessionRevocationDecision>,
}
impl PendingSessionRevocation {
    pub fn from_storage(id: i64, account: AccountId, target: CredentialEpoch, decision: Option<SessionRevocationDecision>) -> Option<Self> {
        if id <= 0 || target == CredentialEpoch::INITIAL {
            return None;
        }
        if let Some(decision) = decision {
            SessionRevocationRequest::from_storage(id, account, target, decision)?;
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
    pub fn request(self) -> Option<SessionRevocationRequest> {
        SessionRevocationRequest::from_storage(self.id, self.account, self.target, self.decision?)
    }
}

/// Persistence witness only: the port must prove a newer CURRENT committed cleanup,
/// and supersession repeats that exact predicate in its own write transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletedSessionRevocation {
    id: i64,
    account: AccountId,
    target: CredentialEpoch,
}
impl CompletedSessionRevocation {
    pub fn from_storage(id: i64, account: AccountId, target: CredentialEpoch) -> Option<Self> {
        (id > 0 && target != CredentialEpoch::INITIAL).then_some(Self { id, account, target })
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRevocationState {
    /// Explicit accepted-upgrade invalidation, not a current cleanup/ACK witness.
    Retired,
    CurrentPending,
    Resolved,
    NewerPending,
    NewerCompleted(CompletedSessionRevocation),
    Invalid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRevocationAcknowledgment {
    Applied,
    AlreadyResolved,
    Changed,
}

/// The concrete owner MUST serialize classify→apply→ack across all finalizers and
/// recovery workers for a store. Otherwise replay after ack can remove fresh sessions.
/// `Retired` requires an accepted stopped-upgrade range and an authoritative matching
/// persisted fact. It is not an acknowledgment or current cleanup witness.
pub trait SessionRevocationRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    type Applied: Send + Sync;
    fn pending(&self, after: i64, limit: NonZeroU8) -> impl Future<Output = Result<Vec<PendingSessionRevocation>, Self::Error>> + Send;
    fn state(&self, pending: &PendingSessionRevocation) -> impl Future<Output = Result<SessionRevocationState, Self::Error>> + Send;
    fn apply(&self, request: SessionRevocationRequest) -> impl Future<Output = Result<Self::Applied, Self::Error>> + Send;
    fn acknowledge(
        &self,
        request: &SessionRevocationRequest,
        applied: &Self::Applied,
    ) -> impl Future<Output = Result<SessionRevocationAcknowledgment, Self::Error>> + Send;
    fn supersede(
        &self,
        pending: &PendingSessionRevocation,
        witness: &CompletedSessionRevocation,
    ) -> impl Future<Output = Result<SessionRevocationAcknowledgment, Self::Error>> + Send;
}
