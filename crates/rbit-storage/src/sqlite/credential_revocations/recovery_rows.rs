//! Checked fixed-width decisions; never print malformed persisted binding bytes.
use crate::StorageError;
use rbit_core::AccountId;
use rbit_identity_models::{
    credential::CredentialEpoch,
    sessions::{CompletedSessionRevocation, PendingSessionRevocation, SessionPromotion, SessionRevocationDecision, SessionRevocationState},
};

#[derive(sqlx::FromRow)]
pub(super) struct CandidateRow {
    pub id: i64,
    pub account_id: i64,
    pub target_epoch: i64,
    pub decision_code: i64,
    pub keep_shape: i64,
    pub keep_spec: Option<Vec<u8>>,
}
impl CandidateRow {
    pub fn checked(self) -> Result<PendingSessionRevocation, StorageError> {
        let account = AccountId(self.account_id);
        let target = CredentialEpoch::new(self.target_epoch).ok_or_else(invalid)?;
        let decision = match (self.decision_code, self.keep_shape, self.keep_spec.as_deref()) {
            (0, 0, None) => None,
            (1, 0, None) => Some(SessionRevocationDecision::EndAll),
            (2, 1, Some(bytes)) => {
                Some(SessionRevocationDecision::KeepCurrent(SessionPromotion::from_storage(account, bytes).ok_or_else(invalid)?))
            }
            _ => return Err(invalid()),
        };
        PendingSessionRevocation::from_storage(self.id, account, target, decision).ok_or_else(invalid)
    }
}
#[derive(sqlx::FromRow)]
pub(super) struct StateRow {
    pub completed_at: Option<i64>,
    pub superseded_by_epoch: Option<i64>,
    pub current_epoch: Option<i64>,
    pub current_id: Option<i64>,
    pub current_decision: i64,
    pub current_outcome: i64,
    pub current_completed: Option<i64>,
    pub current_superseded: Option<i64>,
    pub current_pair_matches: i64,
    pub own_pair_matches: i64,
}
impl StateRow {
    pub fn checked(self, pending: &PendingSessionRevocation) -> Result<SessionRevocationState, StorageError> {
        if self.superseded_by_epoch.is_some() {
            return Ok(SessionRevocationState::Resolved);
        }
        if self.completed_at.is_some() {
            return Ok(if pending.request().is_some() && self.own_pair_matches != 1 {
                SessionRevocationState::Invalid
            } else {
                SessionRevocationState::Resolved
            });
        }
        let current = self.current_epoch.and_then(CredentialEpoch::new).ok_or_else(invalid)?;
        if current == pending.target() {
            return Ok(SessionRevocationState::CurrentPending);
        }
        if current < pending.target() {
            return Ok(SessionRevocationState::Invalid);
        }
        let id = self.current_id.ok_or_else(invalid)?;
        if self.current_superseded.is_some() {
            return Ok(SessionRevocationState::Invalid);
        }
        if self.current_completed.is_none() {
            return Ok(SessionRevocationState::NewerPending);
        }
        let valid = (self.current_decision == 1 && self.current_outcome == 1)
            || (self.current_decision == 2 && matches!(self.current_outcome, 1 | 2));
        if !valid || self.current_pair_matches != 1 {
            return Ok(SessionRevocationState::Invalid);
        }
        let witness = CompletedSessionRevocation::from_storage(id, pending.account(), current).ok_or_else(invalid)?;
        Ok(SessionRevocationState::NewerCompleted(witness))
    }
}
pub(super) fn invalid() -> StorageError {
    StorageError::Corrupt("credential revocation recovery state is invalid".into())
}
