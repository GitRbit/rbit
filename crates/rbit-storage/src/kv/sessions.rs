//! Login sessions keyed by the SHA-256 digest of the session token, plus an
//! account → sessions index so a user can list and revoke their sessions.
//!
//! Storing only the digest means a copy of the database cannot be replayed as cookies.
//! Both tables are written in the same transaction, so the index never drifts from the
//! sessions it points to.

use std::sync::Arc;

use rbit_core::{AccountId, Timestamp};
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use serde::{Deserialize, Serialize};

use crate::{StorageError, error::kv_error};

/// `sha256(token)` → postcard-encoded [`SessionRecord`].
const SESSIONS: TableDefinition<'_, &[u8; 32], &[u8]> = TableDefinition::new("sessions_v2");

/// `(account id, sha256(token))` → nothing: the account's sessions, in key order.
const ACCOUNT_SESSIONS: TableDefinition<'_, (i64, &[u8; 32]), ()> = TableDefinition::new("account_sessions");

/// The fixed-width table used before sessions carried client details. Dropped on open,
/// which signs everyone out once.
const LEGACY_SESSIONS: TableDefinition<'_, &[u8; 32], &[u8; 16]> = TableDefinition::new("sessions");

/// Longest user-agent string kept per session.
const MAX_USER_AGENT_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub account_id: AccountId,
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
    /// Updated at most every few minutes, to keep reads write-free.
    pub last_seen_at: Timestamp,
    /// Client address at sign-in.
    pub ip_address: Option<String>,
    /// The browser's `User-Agent` at sign-in (truncated).
    pub user_agent: String,
}

impl SessionRecord {
    /// A session created now.
    pub fn new(account_id: AccountId, expires_at: Timestamp, ip_address: Option<String>, user_agent: &str) -> Self {
        let now = Timestamp::now();
        let end = user_agent.char_indices().nth(MAX_USER_AGENT_LEN).map_or(user_agent.len(), |(index, _)| index);
        Self { account_id, expires_at, created_at: now, last_seen_at: now, ip_address, user_agent: user_agent[..end].to_owned() }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at <= Timestamp::now()
    }

    fn encode(&self) -> Result<Vec<u8>, StorageError> {
        postcard::to_allocvec(self).map_err(|e| StorageError::Corrupt(format!("session: {e}")))
    }

    /// `None` for undecodable values, which are treated as no session.
    fn decode(bytes: &[u8]) -> Option<Self> {
        postcard::from_bytes(bytes).ok()
    }
}

/// A session together with its key, as listed for its account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub digest: [u8; 32],
    pub record: SessionRecord,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    db: Arc<redb::Database>,
}

impl SessionStore {
    pub(crate) fn new(db: Arc<redb::Database>) -> Self {
        Self { db }
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
        let encoded = record.encode()?;
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            tx.open_table(SESSIONS).map_err(kv_error)?.insert(&digest, encoded.as_slice()).map_err(kv_error)?;
            tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.insert((record.account_id.0, &digest), ()).map_err(kv_error)?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }

    /// Returns the session if it exists and has not expired. Expired sessions are removed lazily.
    pub async fn get(&self, digest: [u8; 32]) -> Result<Option<SessionRecord>, StorageError> {
        let db = Arc::clone(&self.db);
        let record = tokio::task::spawn_blocking(move || -> Result<_, StorageError> {
            let tx = db.begin_read().map_err(kv_error)?;
            let table = tx.open_table(SESSIONS).map_err(kv_error)?;
            Ok(table.get(&digest).map_err(kv_error)?.and_then(|value| SessionRecord::decode(value.value())))
        })
        .await??;

        match record {
            Some(record) if record.is_expired() => {
                self.delete(digest).await?;
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
                let record = table.get(&digest).map_err(kv_error)?.and_then(|value| SessionRecord::decode(value.value()));
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

    /// Deletes every expired session. Intended for a periodic background task.
    pub async fn purge_expired(&self) -> Result<usize, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let now = Timestamp::now();
            let tx = db.begin_write().map_err(kv_error)?;
            let expired: Vec<[u8; 32]> = {
                let table = tx.open_table(SESSIONS).map_err(kv_error)?;
                let mut expired = Vec::new();
                for entry in table.iter().map_err(kv_error)? {
                    let (key, value) = entry.map_err(kv_error)?;
                    if SessionRecord::decode(value.value()).is_none_or(|record| record.expires_at <= now) {
                        expired.push(*key.value());
                    }
                }
                expired
            };
            for digest in &expired {
                remove(&tx, digest)?;
            }
            tx.commit().map_err(kv_error)?;
            Ok(expired.len())
        })
        .await?
    }
}

/// Removes a session and its index entry inside an open write transaction.
fn remove(tx: &redb::WriteTransaction, digest: &[u8; 32]) -> Result<(), StorageError> {
    let mut table = tx.open_table(SESSIONS).map_err(kv_error)?;
    let account = table.remove(digest).map_err(kv_error)?.and_then(|value| SessionRecord::decode(value.value())).map(|r| r.account_id);
    if let Some(account) = account {
        tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.remove((account.0, digest)).map_err(kv_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, SessionStore) {
        let dir = tempfile::tempdir().unwrap();
        let db = redb::Database::create(dir.path().join("kv.redb")).unwrap();
        let store = SessionStore::new(Arc::new(db));
        store.ensure_table().unwrap();
        (dir, store)
    }

    fn record(account: i64, expires_in: i64) -> SessionRecord {
        SessionRecord::new(AccountId(account), Timestamp(Timestamp::now().0 + expires_in), Some("192.0.2.1".into()), "Mozilla/5.0")
    }

    #[test]
    fn record_round_trips_and_truncates_user_agents() {
        let record = SessionRecord::new(AccountId(42), Timestamp(1_900_000_000), None, &"x".repeat(1000));
        assert_eq!(record.user_agent.len(), MAX_USER_AGENT_LEN);
        assert_eq!(SessionRecord::decode(&record.encode().unwrap()), Some(record));
        assert_eq!(SessionRecord::decode(&[0xff; 3]), None);
    }

    #[tokio::test]
    async fn indexes_sessions_by_account() {
        let (_dir, store) = store();
        store.put([1; 32], record(1, 3600)).await.unwrap();
        store.put([2; 32], record(1, 3600)).await.unwrap();
        store.put([3; 32], record(2, 3600)).await.unwrap();
        store.put([4; 32], record(1, -10)).await.unwrap();

        let listed: Vec<_> = store.list_for_account(AccountId(1)).await.unwrap().into_iter().map(|s| s.digest).collect();
        assert_eq!(listed.len(), 2, "expired sessions are not listed");
        assert!(listed.contains(&[1; 32]) && listed.contains(&[2; 32]));

        store.delete([1; 32]).await.unwrap();
        assert_eq!(store.list_for_account(AccountId(1)).await.unwrap().len(), 1);
        assert!(store.get([1; 32]).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn deletes_all_but_one() {
        let (_dir, store) = store();
        for byte in 1..=3 {
            store.put([byte; 32], record(7, 3600)).await.unwrap();
        }
        store.put([9; 32], record(8, 3600)).await.unwrap();

        assert_eq!(store.delete_for_account(AccountId(7), Some([2; 32])).await.unwrap(), 2);
        let left: Vec<_> = store.list_for_account(AccountId(7)).await.unwrap().into_iter().map(|s| s.digest).collect();
        assert_eq!(left, [[2; 32]]);
        assert!(store.get([9; 32]).await.unwrap().is_some(), "other accounts are untouched");

        assert_eq!(store.delete_for_account(AccountId(7), None).await.unwrap(), 1);
        assert_eq!(store.list_for_account(AccountId(7)).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn purges_expired_sessions_and_their_index_entries() {
        let (_dir, store) = store();
        store.put([1; 32], record(1, -1)).await.unwrap();
        store.put([2; 32], record(1, 3600)).await.unwrap();
        assert_eq!(store.purge_expired().await.unwrap(), 1);
        assert_eq!(store.delete_for_account(AccountId(1), None).await.unwrap(), 1, "the purged session left no index entry");
    }
}
