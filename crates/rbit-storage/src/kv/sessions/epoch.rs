//! Dormant epoch-bound redb operations. Legacy session APIs remain unchanged.
//! No SQL lookup, proof verification, promotion or recovery happens in this module.
mod binding;
mod identity;
mod lifecycle;
mod revocation;
mod snapshot;
pub(crate) mod stopped;
pub use revocation::AppliedSessionRevocation;

use super::{ACCOUNT_SESSIONS, SESSIONS, SessionRecord, decode_session, encode_session, remove};
use crate::{StorageError, error::kv_error};
use binding::{Binding, corrupt};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::CredentialEpoch;
use redb::{ReadableDatabase, ReadableTable, TableDefinition};
pub use snapshot::EpochSnapshot;
use snapshot::same_identity;
use std::sync::Arc;

/// Fixed 32-byte value: account i64 BE, epoch i64 BE, random 16-byte incarnation.
const EPOCHS: TableDefinition<'_, &[u8; 32], &[u8; 32]> = TableDefinition::new("session_epochs_v1");

#[derive(Clone)]
pub struct EpochSessionStore {
    db: Arc<redb::Database>,
    random: fn(&mut [u8]) -> Result<(), ()>,
}
impl std::fmt::Debug for EpochSessionStore {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("EpochSessionStore").finish_non_exhaustive()
    }
}
impl EpochSessionStore {
    pub(super) fn new(db: Arc<redb::Database>) -> Self {
        Self { db, random: |bytes| getrandom::fill(bytes).map_err(|_| ()) }
    }

    /// Insert-only: never replace a digest's record, matching index, or orphan binding.
    /// `original` comes from the eventual proof owner, never a fresh SQL epoch read here.
    /// Random-source failure occurs before opening a write transaction.
    pub async fn put_if_absent(
        &self,
        digest: [u8; 32],
        record: SessionRecord,
        account: AccountId,
        original: CredentialEpoch,
    ) -> Result<bool, StorageError> {
        Ok(self.put_bound(digest, record, account, original).await?.is_some())
    }

    /// Same insertion transaction returns its original captured binding; no follow-up
    /// read may adopt a replacement. Still dormant, without SQL or mode authority.
    pub async fn put_bound(
        &self,
        digest: [u8; 32],
        record: SessionRecord,
        account: AccountId,
        original: CredentialEpoch,
    ) -> Result<Option<EpochSnapshot>, StorageError> {
        if record.account_id != account {
            return Ok(None);
        }
        let binding = Binding::with_random(account, original, self.random)?;
        let encoded = encode_session(&record)?;
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            if !insert(&tx, digest, &record, &encoded, binding)? {
                return Ok(None);
            }
            tx.commit().map_err(kv_error)?;
            Ok(Some(EpochSnapshot { digest, record, binding: Some(binding) }))
        })
        .await?
    }

    /// One read transaction snapshots record + sidecar. The derived account index is
    /// not scanned on this hot path. Missing sidecars yield explicit legacy INITIAL;
    /// such snapshots cannot mutate or adopt an epoch through these methods.
    pub async fn get_live(&self, digest: [u8; 32]) -> Result<Option<EpochSnapshot>, StorageError> {
        Ok(self.get_snapshot(digest).await?.filter(|snapshot| !snapshot.record.is_expired()))
    }

    /// Captures expired records too, so logout/maintenance can compare their actual
    /// incarnation. Missing sidecars stay explicit legacy INITIAL, never adopted.
    pub async fn get_snapshot(&self, digest: [u8; 32]) -> Result<Option<EpochSnapshot>, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            let table = tx.open_table(SESSIONS).map_err(kv_error)?;
            let Some(record) = table.get(&digest).map_err(kv_error)?.and_then(|value| decode_session(value.value())) else {
                return Ok(None);
            };
            let binding = match tx.open_table(EPOCHS) {
                Ok(epochs) => {
                    epochs.get(&digest).map_err(kv_error)?.map(|value| Binding::decode(value.value(), record.account_id)).transpose()?
                }
                Err(redb::TableError::TableDoesNotExist(_)) => None,
                Err(error) => return Err(kv_error(error)),
            };
            Ok(Some(EpochSnapshot { digest, record, binding }))
        })
        .await?
    }

    /// Fixed captured binding, live-record recheck and monotonic timestamps, without
    /// upsert, sidecar adoption, epoch refresh or revoked-session resurrection.
    pub async fn renew(&self, expected: &EpochSnapshot, expires: Timestamp, seen: Timestamp) -> Result<bool, StorageError> {
        self.renew_captured(expected.clone(), expires, seen).await
    }

    /// Consuming form for an already-owned finalization, without a second metadata clone.
    pub async fn renew_captured(&self, expected: EpochSnapshot, expires: Timestamp, seen: Timestamp) -> Result<bool, StorageError> {
        let Some(binding) = expected.binding else { return Ok(false) };
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            let Some(mut current) = matched(&tx, &expected, binding)? else { return Ok(false) };
            if current.is_expired() {
                return Ok(false);
            }
            current.expires_at = current.expires_at.max(expires);
            current.last_seen_at = current.last_seen_at.max(seen);
            tx.open_table(SESSIONS).map_err(kv_error)?.insert(&expected.digest, encode_session(&current)?.as_slice()).map_err(kv_error)?;
            tx.commit().map_err(kv_error)?;
            Ok(true)
        })
        .await?
    }

    /// Delete only the captured incarnation. A late logout cannot delete a new record
    /// reusing the same digest/account/epoch. Missing-sidecar snapshots refuse mutation.
    pub async fn delete(&self, expected: &EpochSnapshot) -> Result<bool, StorageError> {
        self.delete_captured(expected.clone()).await
    }

    /// Consuming form for an already-owned logout/revocation.
    pub async fn delete_captured(&self, expected: EpochSnapshot) -> Result<bool, StorageError> {
        let Some(binding) = expected.binding else { return Ok(false) };
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            if matched(&tx, &expected, binding)?.is_none() {
                return Ok(false);
            }
            remove(&tx, &expected.digest)?;
            tx.open_table(EPOCHS).map_err(kv_error)?.remove(&expected.digest).map_err(kv_error)?;
            tx.commit().map_err(kv_error)?;
            Ok(true)
        })
        .await?
    }
}

fn insert(
    tx: &redb::WriteTransaction,
    digest: [u8; 32],
    record: &SessionRecord,
    encoded: &[u8],
    binding: Binding,
) -> Result<bool, StorageError> {
    if tx.open_table(SESSIONS).map_err(kv_error)?.get(&digest).map_err(kv_error)?.is_some()
        || tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.get((record.account_id.0, &digest)).map_err(kv_error)?.is_some()
        || tx.open_table(EPOCHS).map_err(kv_error)?.get(&digest).map_err(kv_error)?.is_some()
    {
        return Ok(false);
    }
    tx.open_table(SESSIONS).map_err(kv_error)?.insert(&digest, encoded).map_err(kv_error)?;
    tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.insert((record.account_id.0, &digest), ()).map_err(kv_error)?;
    tx.open_table(EPOCHS).map_err(kv_error)?.insert(&digest, &binding.encode()).map_err(kv_error)?;
    Ok(true)
}

fn matched(tx: &redb::WriteTransaction, expected: &EpochSnapshot, binding: Binding) -> Result<Option<SessionRecord>, StorageError> {
    let Some(current) =
        tx.open_table(SESSIONS).map_err(kv_error)?.get(&expected.digest).map_err(kv_error)?.and_then(|value| decode_session(value.value()))
    else {
        return Ok(None);
    };
    if !same_identity(&current, &expected.record) {
        return Ok(None);
    }
    let epochs = tx.open_table(EPOCHS).map_err(kv_error)?;
    let Some(value) = epochs.get(&expected.digest).map_err(kv_error)? else { return Ok(None) };
    if Binding::decode(value.value(), current.account_id)? != binding {
        return Ok(None);
    }
    if tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.get((current.account_id.0, &expected.digest)).map_err(kv_error)?.is_none() {
        return Err(corrupt());
    }
    Ok(Some(current))
}

#[cfg(test)]
mod tests;
