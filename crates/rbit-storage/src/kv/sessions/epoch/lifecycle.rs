//! Epoch-bound listing, revocation and bounded maintenance. No legacy mutations.
use super::super::PURGE_BATCH_SIZE;
use super::{ACCOUNT_SESSIONS, Binding, EPOCHS, EpochSessionStore, EpochSnapshot, SESSIONS, corrupt, decode_session, matched, remove};
use crate::{StorageError, error::kv_error};
use rbit_core::{AccountId, Timestamp};
use redb::{ReadableDatabase, ReadableTable};
use std::{ops::Bound, sync::Arc};

impl EpochSessionStore {
    pub async fn list_bound(&self, account: AccountId) -> Result<Vec<EpochSnapshot>, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            let records = tx.open_table(SESSIONS).map_err(kv_error)?;
            let index = tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?;
            let epochs = match tx.open_table(EPOCHS) {
                Ok(table) => Some(table),
                Err(redb::TableError::TableDoesNotExist(_)) => None,
                Err(error) => return Err(kv_error(error)),
            };
            let mut snapshots = Vec::new();
            for entry in index.range((account.0, &[0_u8; 32])..=(account.0, &[0xff_u8; 32])).map_err(kv_error)? {
                let (key, _) = entry.map_err(kv_error)?;
                let digest = *key.value().1;
                let record = records.get(&digest).map_err(kv_error)?.and_then(|value| decode_session(value.value())).ok_or_else(corrupt)?;
                if record.account_id != account {
                    return Err(corrupt());
                }
                let binding = epochs.as_ref().ok_or_else(corrupt)?.get(&digest).map_err(kv_error)?.ok_or_else(corrupt)?;
                let binding = Binding::decode(binding.value(), account)?;
                if !record.is_expired() {
                    snapshots.push(EpochSnapshot { digest, record, binding: Some(binding) });
                }
            }
            snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.record.last_seen_at));
            Ok(snapshots)
        })
        .await?
    }

    /// One write transaction linearizes current-binding validation and all other
    /// same-epoch deletions. Newer account epochs are never removed by an old request.
    pub async fn revoke_others(&self, current: &EpochSnapshot) -> Result<Option<usize>, StorageError> {
        self.revoke_others_captured(current.clone()).await
    }

    pub async fn revoke_others_captured(&self, current: EpochSnapshot) -> Result<Option<usize>, StorageError> {
        let Some(binding) = current.binding else { return Ok(None) };
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            if matched(&tx, &current, binding)?.is_none_or(|record| record.is_expired()) {
                return Ok(None);
            }
            let mut after = None;
            let mut removed = 0;
            loop {
                let batch = account_batch(&tx, current.record.account_id, after)?;
                if batch.is_empty() {
                    break;
                }
                after = batch.last().copied();
                for digest in batch {
                    let candidate = captured(&tx, digest)?;
                    if candidate.record.account_id != current.record.account_id {
                        return Err(corrupt());
                    }
                    if candidate.epoch() == binding.epoch && digest != current.digest {
                        remove_bound(&tx, &digest)?;
                        removed += 1;
                    }
                }
            }
            tx.commit().map_err(kv_error)?;
            Ok(Some(removed))
        })
        .await?
    }

    /// Same bounded batch size as legacy maintenance. Every deletion checks the
    /// captured incarnation and current expiry after renewal/replacement races.
    pub async fn purge_expired_bound(&self) -> Result<usize, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let now = Timestamp::now();
            let read = db.begin_read().map_err(kv_error)?;
            let records = read.open_table(SESSIONS).map_err(kv_error)?;
            let epochs = match read.open_table(EPOCHS) {
                Ok(table) => Some(table),
                Err(redb::TableError::TableDoesNotExist(_)) => None,
                Err(error) => return Err(kv_error(error)),
            };
            let mut batch = Vec::with_capacity(PURGE_BATCH_SIZE);
            let mut removed = 0;
            for entry in records.iter().map_err(kv_error)? {
                let (key, value) = entry.map_err(kv_error)?;
                let record = decode_session(value.value()).ok_or_else(corrupt)?;
                if record.expires_at > now {
                    continue;
                }
                let digest = *key.value();
                let binding = epochs.as_ref().ok_or_else(corrupt)?.get(&digest).map_err(kv_error)?.ok_or_else(corrupt)?;
                let binding = Binding::decode(binding.value(), record.account_id)?;
                batch.push(EpochSnapshot { digest, record, binding: Some(binding) });
                if batch.len() == PURGE_BATCH_SIZE {
                    removed += purge_batch(&db, &batch, now)?;
                    batch.clear();
                }
            }
            removed += purge_batch(&db, &batch, now)?;
            Ok(removed)
        })
        .await?
    }
}

pub(super) fn account_batch(
    tx: &redb::WriteTransaction,
    account: AccountId,
    after: Option<[u8; 32]>,
) -> Result<Vec<[u8; 32]>, StorageError> {
    let index = tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?;
    let zero = [0_u8; 32];
    let max = [0xff_u8; 32];
    let lower = match after.as_ref() {
        Some(digest) => Bound::Excluded((account.0, digest)),
        None => Bound::Included((account.0, &zero)),
    };
    index
        .range((lower, Bound::Included((account.0, &max))))
        .map_err(kv_error)?
        .take(PURGE_BATCH_SIZE)
        .map(|entry| entry.map(|(key, _)| *key.value().1).map_err(kv_error))
        .collect()
}

pub(super) fn captured(tx: &redb::WriteTransaction, digest: [u8; 32]) -> Result<EpochSnapshot, StorageError> {
    let records = tx.open_table(SESSIONS).map_err(kv_error)?;
    let record = records.get(&digest).map_err(kv_error)?.and_then(|value| decode_session(value.value())).ok_or_else(corrupt)?;
    let epochs = tx.open_table(EPOCHS).map_err(kv_error)?;
    let binding = epochs.get(&digest).map_err(kv_error)?.ok_or_else(corrupt)?;
    let binding = Binding::decode(binding.value(), record.account_id)?;
    Ok(EpochSnapshot { digest, record, binding: Some(binding) })
}

pub(super) fn remove_bound(tx: &redb::WriteTransaction, digest: &[u8; 32]) -> Result<(), StorageError> {
    remove(tx, digest)?;
    tx.open_table(EPOCHS).map_err(kv_error)?.remove(digest).map_err(kv_error)?;
    Ok(())
}

pub(super) fn purge_batch(db: &redb::Database, batch: &[EpochSnapshot], now: Timestamp) -> Result<usize, StorageError> {
    if batch.is_empty() {
        return Ok(0);
    }
    let tx = db.begin_write().map_err(kv_error)?;
    let mut removed = 0;
    for expected in batch {
        let binding = expected.binding.ok_or_else(corrupt)?;
        if matched(&tx, expected, binding)?.is_some_and(|record| record.expires_at <= now) {
            remove_bound(&tx, &expected.digest)?;
            removed += 1;
        }
    }
    tx.commit().map_err(kv_error)?;
    Ok(removed)
}
