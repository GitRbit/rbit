//! Login sessions keyed by the SHA-256 digest of the session token, plus an
//! account → sessions index so a user can list and revoke their sessions.
//!
//! Storing only the digest means a copy of the database cannot be replayed as cookies.
//! Both tables are written in the same transaction, so the index never drifts from the
//! sessions it points to.

mod epoch;

pub use epoch::{EpochSessionStore, EpochSnapshot};

use std::sync::Arc;

use rbit_core::{AccountId, Timestamp};
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};

use crate::{StorageError, error::kv_error};

/// `sha256(token)` → postcard-encoded [`SessionRecord`].
const SESSIONS: TableDefinition<'_, &[u8; 32], &[u8]> = TableDefinition::new("sessions_v2");

/// `(account id, sha256(token))` → nothing: the account's sessions, in key order.
const ACCOUNT_SESSIONS: TableDefinition<'_, (i64, &[u8; 32]), ()> = TableDefinition::new("account_sessions");

/// The fixed-width table used before sessions carried client details. Dropped on open,
/// which signs everyone out once.
const LEGACY_SESSIONS: TableDefinition<'_, &[u8; 32], &[u8; 16]> = TableDefinition::new("sessions");

/// Expiry scanning is read-only; each cleanup transaction touches at most this many keys.
const PURGE_BATCH_SIZE: usize = 256;

pub use rbit_identity_models::sessions::{SessionRecord, StoredSession};

#[derive(Debug, Clone)]
pub struct SessionStore {
    db: Arc<redb::Database>,
}

impl SessionStore {
    pub(crate) fn new(db: Arc<redb::Database>) -> Self {
        Self { db }
    }

    /// Inactive alternate persistence seam; no tables or production behavior change
    /// until explicitly used. Mixing it with legacy mutations is not activation-safe.
    pub fn epoch_sidecar(&self) -> EpochSessionStore {
        EpochSessionStore::new(Arc::clone(&self.db))
    }

    /// Creates the tables so read transactions never fail on a fresh database, and drops
    /// the pre-index table.
    pub(crate) fn ensure_table(&self) -> Result<(), StorageError> {
        let tx = self.db.begin_write().map_err(kv_error)?;
        tx.open_table(SESSIONS).map_err(kv_error)?;
        tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?;
        if tx.list_tables().map_err(kv_error)?.any(|table| table.name() == LEGACY_SESSIONS.name()) {
            tx.delete_table(LEGACY_SESSIONS).map_err(kv_error)?;
        }
        tx.commit().map_err(kv_error)
    }

    /// Inserts or replaces a session (and indexes it under its account).
    pub async fn put(&self, digest: [u8; 32], record: SessionRecord) -> Result<(), StorageError> {
        let encoded = encode_session(&record)?;
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            remove(&tx, &digest)?;
            tx.open_table(SESSIONS).map_err(kv_error)?.insert(&digest, encoded.as_slice()).map_err(kv_error)?;
            tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.insert((record.account_id.0, &digest), ()).map_err(kv_error)?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }

    /// Renews a live session atomically, without recreating a revoked or expired one.
    /// Concurrent requests can only move its timestamps forward.
    pub async fn renew(&self, digest: [u8; 32], expires_at: Timestamp, last_seen_at: Timestamp) -> Result<bool, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            {
                let mut table = tx.open_table(SESSIONS).map_err(kv_error)?;
                let record = table.get(&digest).map_err(kv_error)?.and_then(|value| decode_session(value.value()));
                let Some(mut record) = record.filter(|record| !record.is_expired()) else {
                    return Ok(false);
                };
                record.expires_at = record.expires_at.max(expires_at);
                record.last_seen_at = record.last_seen_at.max(last_seen_at);
                table.insert(&digest, encode_session(&record)?.as_slice()).map_err(kv_error)?;
            }
            tx.commit().map_err(kv_error)?;
            Ok(true)
        })
        .await?
    }

    /// Returns the session if it exists and has not expired. Expired sessions are removed lazily.
    pub async fn get(&self, digest: [u8; 32]) -> Result<Option<SessionRecord>, StorageError> {
        let db = Arc::clone(&self.db);
        let record = tokio::task::spawn_blocking(move || -> Result<_, StorageError> {
            let tx = db.begin_read().map_err(kv_error)?;
            let table = tx.open_table(SESSIONS).map_err(kv_error)?;
            Ok(table.get(&digest).map_err(kv_error)?.and_then(|value| decode_session(value.value())))
        })
        .await??;

        match record {
            Some(record) if record.is_expired() => {
                let db = Arc::clone(&self.db);
                tokio::task::spawn_blocking(move || {
                    let tx = db.begin_write().map_err(kv_error)?;
                    let expired = tx
                        .open_table(SESSIONS)
                        .map_err(kv_error)?
                        .get(&digest)
                        .map_err(kv_error)?
                        .and_then(|value| decode_session(value.value()))
                        .is_none_or(|record| record.is_expired());
                    if expired {
                        remove(&tx, &digest)?;
                    }
                    tx.commit().map_err(kv_error)
                })
                .await??;
                Ok(None)
            }
            other => Ok(other),
        }
    }

    pub async fn delete(&self, digest: [u8; 32]) -> Result<(), StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            remove(&tx, &digest)?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }

    /// The account's unexpired sessions, most recently active first.
    pub async fn list_for_account(&self, account: AccountId) -> Result<Vec<StoredSession>, StorageError> {
        let db = Arc::clone(&self.db);
        let mut sessions = tokio::task::spawn_blocking(move || -> Result<_, StorageError> {
            let tx = db.begin_read().map_err(kv_error)?;
            let index = tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?;
            let table = tx.open_table(SESSIONS).map_err(kv_error)?;
            let mut sessions = Vec::new();
            for entry in index.range((account.0, &[0_u8; 32])..=(account.0, &[0xff_u8; 32])).map_err(kv_error)? {
                let (key, ()) = entry.map(|(key, value)| (key, value.value())).map_err(kv_error)?;
                let digest = *key.value().1;
                let record = table.get(&digest).map_err(kv_error)?.and_then(|value| decode_session(value.value()));
                if let Some(record) = record.filter(|record| !record.is_expired()) {
                    sessions.push(StoredSession { digest, record });
                }
            }
            Ok(sessions)
        })
        .await??;
        sessions.sort_by_key(|session| std::cmp::Reverse(session.record.last_seen_at));
        Ok(sessions)
    }

    /// Deletes every session of `account` except `keep`; returns how many were removed.
    pub async fn delete_for_account(&self, account: AccountId, keep: Option<[u8; 32]>) -> Result<usize, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            let digests: Vec<[u8; 32]> = {
                let index = tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?;
                index
                    .range((account.0, &[0_u8; 32])..=(account.0, &[0xff_u8; 32]))
                    .map_err(kv_error)?
                    .map(|entry| entry.map(|(key, _)| *key.value().1).map_err(kv_error))
                    .collect::<Result<_, _>>()?
            };
            let mut removed = 0;
            for digest in digests.iter().filter(|digest| Some(**digest) != keep) {
                remove(&tx, digest)?;
                removed += 1;
            }
            tx.commit().map_err(kv_error)?;
            Ok(removed)
        })
        .await?
    }

    /// Deletes expired sessions from a read snapshot, rechecking each candidate under
    /// a short write transaction. Active sessions never hold up other database writers
    /// while they are scanned; renewal or replacement after the snapshot remains safe.
    pub async fn purge_expired(&self) -> Result<usize, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let now = Timestamp::now();
            let snapshot = db.begin_read().map_err(kv_error)?;
            let table = snapshot.open_table(SESSIONS).map_err(kv_error)?;
            let mut expired = Vec::with_capacity(PURGE_BATCH_SIZE);
            let mut removed = 0;
            for entry in table.iter().map_err(kv_error)? {
                let (key, value) = entry.map_err(kv_error)?;
                if decode_session(value.value()).is_none_or(|record| record.expires_at <= now) {
                    expired.push(*key.value());
                    if expired.len() == PURGE_BATCH_SIZE {
                        removed += remove_expired_batch(&db, &expired, now)?;
                        expired.clear();
                    }
                }
            }
            removed += remove_expired_batch(&db, &expired, now)?;
            Ok(removed)
        })
        .await?
    }
}

/// A snapshot may contain keys that another request has replaced or revoked.
/// Check their current value atomically with removal, and count only existing rows.
fn remove_expired_batch(db: &redb::Database, digests: &[[u8; 32]], now: Timestamp) -> Result<usize, StorageError> {
    if digests.is_empty() {
        return Ok(0);
    }
    let tx = db.begin_write().map_err(kv_error)?;
    let mut removed = 0;
    for digest in digests {
        let expired = tx
            .open_table(SESSIONS)
            .map_err(kv_error)?
            .get(digest)
            .map_err(kv_error)?
            .is_some_and(|value| decode_session(value.value()).is_none_or(|record| record.expires_at <= now));
        if expired {
            remove(&tx, digest)?;
            removed += 1;
        }
    }
    tx.commit().map_err(kv_error)?;
    Ok(removed)
}

/// Removes a session and its index entry inside an open write transaction.
fn remove(tx: &redb::WriteTransaction, digest: &[u8; 32]) -> Result<(), StorageError> {
    let mut table = tx.open_table(SESSIONS).map_err(kv_error)?;
    let account = table.remove(digest).map_err(kv_error)?.and_then(|value| decode_session(value.value())).map(|r| r.account_id);
    if let Some(account) = account {
        tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.remove((account.0, digest)).map_err(kv_error)?;
    }
    Ok(())
}

fn encode_session(record: &SessionRecord) -> Result<Vec<u8>, StorageError> {
    postcard::to_allocvec(record).map_err(|e| StorageError::Corrupt(format!("session: {e}")))
}

fn decode_session(bytes: &[u8]) -> Option<SessionRecord> {
    postcard::from_bytes(bytes).ok()
}

#[cfg(test)]
mod tests;
