//! Concrete bounded durable recovery queries, not an activated background worker.
#[path = "recovery_pair.rs"]
mod pair;
#[path = "recovery_queries.rs"]
mod queries;
#[path = "recovery_rows.rs"]
mod rows;
use super::super::pool::Pool;
use super::RevocationIntent;
use crate::{AppliedSessionRevocation, EpochSessionStore, StorageError};
use rbit_core::Timestamp;
use rbit_identity_models::sessions::{
    CompletedSessionRevocation, PendingSessionRevocation, SessionRevocationAcknowledgment, SessionRevocationDecision,
    SessionRevocationOutcome, SessionRevocationRequest, SessionRevocationState,
};
use std::num::NonZeroU8;

#[derive(Clone, Debug)]
pub struct CredentialRevocationStore {
    pool: Pool,
}
impl CredentialRevocationStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }
    pub async fn pending(&self, after: i64, limit: NonZeroU8) -> Result<Vec<PendingSessionRevocation>, StorageError> {
        let rows: Vec<rows::CandidateRow> =
            sqlx::query_as(queries::PENDING).bind(after).bind(i64::from(limit.get())).fetch_all(&self.pool).await?;
        rows.into_iter().map(rows::CandidateRow::checked).collect()
    }
    pub async fn find(&self, intent: &RevocationIntent) -> Result<Option<PendingSessionRevocation>, StorageError> {
        let row: Option<rows::CandidateRow> = sqlx::query_as(queries::FIND)
            .bind(intent.id)
            .bind(intent.account_id.0)
            .bind(intent.target_epoch)
            .fetch_optional(&self.pool)
            .await?;
        row.map(rows::CandidateRow::checked).transpose()
    }
    pub async fn state(&self, pending: &PendingSessionRevocation) -> Result<SessionRevocationState, StorageError> {
        let row: Option<rows::StateRow> = sqlx::query_as(queries::STATE)
            .bind(pending.id())
            .bind(pending.account().0)
            .bind(pending.target().get())
            .fetch_optional(&self.pool)
            .await?;
        row.map_or(Ok(SessionRevocationState::Resolved), |row| row.checked(pending))
    }
    /// Only a completion from the SAME actual redb owner and exact durable request
    /// can reach the SQL acknowledgment. No proof is reconstructed from currentUser.
    pub async fn acknowledge(
        &self,
        request: &SessionRevocationRequest,
        store: &EpochSessionStore,
        applied: &AppliedSessionRevocation,
        now: Timestamp,
    ) -> Result<SessionRevocationAcknowledgment, StorageError> {
        if !applied.matches(store, request) {
            return Err(rows::invalid());
        }
        let (policy, keep) = decision_parts(request.decision());
        let outcome = match applied.outcome() {
            SessionRevocationOutcome::EndedAll => "ended_all",
            SessionRevocationOutcome::KeptCurrent => "kept_current",
        };
        let pair = applied.pair_identity().storage_bytes();
        let changed = sqlx::query(queries::ACK)
            .bind(request.id())
            .bind(request.account().0)
            .bind(request.target().get())
            .bind(policy)
            .bind(keep.as_ref().map(|bytes| bytes.as_slice()))
            .bind(outcome)
            .bind(now.seconds())
            .bind(pair.as_slice())
            .execute(&self.pool)
            .await?
            .rows_affected();
        if changed != 0 {
            return Ok(SessionRevocationAcknowledgment::Applied);
        }
        let pending = PendingSessionRevocation::from_storage(request.id(), request.account(), request.target(), Some(request.decision()))
            .ok_or_else(rows::invalid)?;
        self.changed(&pending).await
    }
    /// Metadata retirement only after a newer CURRENT committed cleanup; never just
    /// because the account epoch advanced or another intent is still pending.
    pub async fn supersede(
        &self,
        pending: &PendingSessionRevocation,
        witness: &CompletedSessionRevocation,
    ) -> Result<SessionRevocationAcknowledgment, StorageError> {
        if witness.account() != pending.account() || witness.target() <= pending.target() {
            return Err(rows::invalid());
        }
        let changed = sqlx::query(queries::SUPERSEDE)
            .bind(pending.id())
            .bind(pending.account().0)
            .bind(pending.target().get())
            .bind(witness.target().get())
            .bind(witness.id())
            .execute(&self.pool)
            .await?
            .rows_affected();
        if changed != 0 {
            return Ok(SessionRevocationAcknowledgment::Applied);
        }
        self.changed(pending).await
    }
    async fn changed(&self, pending: &PendingSessionRevocation) -> Result<SessionRevocationAcknowledgment, StorageError> {
        Ok(if self.state(pending).await? == SessionRevocationState::Resolved {
            SessionRevocationAcknowledgment::AlreadyResolved
        } else {
            SessionRevocationAcknowledgment::Changed
        })
    }
}
pub(crate) fn decision_parts(decision: SessionRevocationDecision) -> (&'static str, Option<[u8; 64]>) {
    match decision {
        SessionRevocationDecision::EndAll => ("end_all", None),
        SessionRevocationDecision::KeepCurrent(spec) => ("keep_current", Some(spec.storage_bytes())),
    }
}
