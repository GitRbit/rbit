//! Exact durable decision replay. Call only under the composed pending-intent owner.
use super::lifecycle::{account_batch, captured, remove_bound};
use super::{ACCOUNT_SESSIONS, Binding, EPOCHS, EpochSessionStore, EpochSnapshot, SESSIONS, corrupt, decode_session, same_identity};
use crate::{StorageError, error::kv_error};
use rbit_identity_models::sessions::{
    SessionPairIdentity, SessionPromotion, SessionRevocationDecision, SessionRevocationOutcome, SessionRevocationRequest,
};
use redb::ReadableTable;
use std::sync::{Arc, Weak};

/// Backend-issued completion data. No constructor/Serde or raw binding Debug.
/// It proves this particular redb commit, not SQL acknowledgment or crash durability
/// beyond redb, and is not authority for another store instance/composition.
pub struct AppliedSessionRevocation {
    owner: Weak<redb::Database>,
    request: SessionRevocationRequest,
    outcome: SessionRevocationOutcome,
    pair: SessionPairIdentity,
    #[cfg(test)]
    mode: super::runtime::SessionMode,
}
impl AppliedSessionRevocation {
    pub(crate) fn pair_identity(&self) -> SessionPairIdentity {
        self.pair
    }
    pub fn outcome(&self) -> SessionRevocationOutcome {
        self.outcome
    }
    pub(crate) fn matches(&self, store: &EpochSessionStore, request: &SessionRevocationRequest) -> bool {
        #[cfg(test)]
        if self.mode != store.mode {
            return false;
        }
        Weak::ptr_eq(&self.owner, &Arc::downgrade(&store.db)) && self.request == *request
    }
}
impl std::fmt::Debug for AppliedSessionRevocation {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("AppliedSessionRevocation").field("outcome", &self.outcome).finish_non_exhaustive()
    }
}
impl EpochSessionStore {
    /// Requires the composed owner to serialize classify→apply→ack and prove this
    /// exact intent is CURRENT/PENDING. Replaying after ack can remove newly issued
    /// same-target sessions; an arbitrary persistence request is not such authority.
    pub async fn apply_pending_revocation(
        &self,
        request: SessionRevocationRequest,
        expected_pair: SessionPairIdentity,
    ) -> Result<AppliedSessionRevocation, StorageError> {
        #[cfg(test)]
        let mode = self.mode;
        let db = Arc::clone(&self.db);
        let owner = Arc::downgrade(&db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            #[cfg(test)]
            let tx = mode.prepare(tx)?;
            let pair = super::identity::in_write(&tx)?;
            if pair != expected_pair {
                return Err(corrupt());
            }
            let keep = match request.decision() {
                SessionRevocationDecision::EndAll => None,
                SessionRevocationDecision::KeepCurrent(spec) => live_promotion(&tx, spec, request.target())?,
            };
            let mut after = None;
            loop {
                let batch = account_batch(&tx, request.account(), after)?;
                if batch.is_empty() {
                    break;
                }
                after = batch.last().copied();
                for digest in batch {
                    let candidate = captured(&tx, digest)?;
                    if candidate.record.account_id != request.account() {
                        return Err(corrupt());
                    }
                    if candidate.epoch() <= request.target() && keep.as_ref().is_none_or(|snapshot| snapshot.digest != digest) {
                        remove_bound(&tx, &digest)?;
                    }
                }
            }
            let outcome = if let Some(keep) = keep {
                let promoted = keep.binding.ok_or_else(corrupt)?.promoted(request.target());
                tx.open_table(EPOCHS).map_err(kv_error)?.insert(&keep.digest, &promoted.encode()).map_err(kv_error)?;
                SessionRevocationOutcome::KeptCurrent
            } else {
                SessionRevocationOutcome::EndedAll
            };
            #[cfg(test)]
            mode.advance(&tx, request.target())?;
            tx.commit().map_err(kv_error)?;
            Ok(AppliedSessionRevocation {
                owner,
                request,
                outcome,
                pair,
                #[cfg(test)]
                mode,
            })
        })
        .await?
    }

    pub async fn logout(&self, expected: &EpochSnapshot) -> Result<bool, StorageError> {
        self.logout_captured(expected.clone()).await
    }
    /// The cookie holder may revoke the SAME incarnation after a keep promotion;
    /// stale resolver cleanup still uses epoch-exact delete, never this operation.
    pub async fn logout_captured(&self, expected: EpochSnapshot) -> Result<bool, StorageError> {
        let Some(expected_binding) = expected.binding else { return Ok(false) };
        #[cfg(test)]
        let mode = self.mode;
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            #[cfg(test)]
            let tx = mode.prepare(tx)?;
            if tx.open_table(SESSIONS).map_err(kv_error)?.get(&expected.digest).map_err(kv_error)?.is_none() {
                return Ok(false);
            }
            let current = captured(&tx, expected.digest)?;
            let binding = current.binding.ok_or_else(corrupt)?;
            if !same_identity(&current.record, &expected.record)
                || !binding.same_incarnation(expected_binding)
                || binding.epoch < expected_binding.epoch
            {
                return Ok(false);
            }
            if tx
                .open_table(ACCOUNT_SESSIONS)
                .map_err(kv_error)?
                .get((current.record.account_id.0, &expected.digest))
                .map_err(kv_error)?
                .is_none()
            {
                return Err(corrupt());
            }
            remove_bound(&tx, &expected.digest)?;
            #[cfg(test)]
            mode.advance(&tx, binding.epoch)?;
            tx.commit().map_err(kv_error)?;
            Ok(true)
        })
        .await?
    }
}

fn live_promotion(
    tx: &redb::WriteTransaction,
    spec: SessionPromotion,
    target: rbit_identity_models::credential::CredentialEpoch,
) -> Result<Option<EpochSnapshot>, StorageError> {
    let digest = spec.digest();
    let records = tx.open_table(SESSIONS).map_err(kv_error)?;
    let Some(value) = records.get(&digest).map_err(kv_error)? else {
        if tx.open_table(EPOCHS).map_err(kv_error)?.get(&digest).map_err(kv_error)?.is_some()
            || tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.get((spec.account().0, &digest)).map_err(kv_error)?.is_some()
        {
            return Err(corrupt());
        }
        return Ok(None);
    };
    let record = decode_session(value.value()).ok_or_else(corrupt)?;
    if record.account_id != spec.account() {
        return Ok(None);
    }
    let bytes = spec.storage_bytes();
    let original = Binding::decode(bytes[32..].try_into().map_err(|_| corrupt())?, spec.account())?;
    let epochs = tx.open_table(EPOCHS).map_err(kv_error)?;
    let binding = epochs.get(&digest).map_err(kv_error)?.ok_or_else(corrupt)?;
    let binding = Binding::decode(binding.value(), record.account_id)?;
    if binding != original && binding != original.promoted(target) {
        return Ok(None);
    }
    if record.is_expired() {
        return Ok(None);
    }
    if tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.get((record.account_id.0, &digest)).map_err(kv_error)?.is_none() {
        return Err(corrupt());
    }
    Ok(Some(EpochSnapshot { digest, record, binding: Some(binding) }))
}
