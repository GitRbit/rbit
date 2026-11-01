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
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

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

/// Expiry scanning is read-only; each cleanup transaction touches at most this many keys.
const PURGE_BATCH_SIZE: usize = 256;

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
    writer: rbit_authority_fence::WriterRegistration,
    purge: Arc<Semaphore>,
}

impl SessionStore {
    pub(crate) fn new(db: Arc<redb::Database>, writer: rbit_authority_fence::WriterRegistration, purge: Arc<Semaphore>) -> Self {
        Self { db, writer, purge }
    }

    pub(super) fn is_bound_to(&self, writer: &rbit_authority_fence::WriterRegistration) -> bool {
        self.writer.same_domain(writer)
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

    // Admission precedes submission; the actual queued/running worker owns the
    // mutation until its complete synchronous result, independently of its waiter.
    async fn write<T, F>(&self, operation: F) -> Result<T, StorageError>
    where
        T: Send + 'static,
        F: FnOnce(&redb::Database) -> Result<T, StorageError> + Send + 'static,
    {
        let mutation = self.writer.begin().await.map_err(session_mutation_error)?;
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let result = operation(&db);
            if result.is_ok() {
                mutation.complete();
            }
            result
        })
        .await?
    }

    /// Inserts or replaces a session (and indexes it under its account).
    pub async fn put(&self, digest: [u8; 32], record: SessionRecord) -> Result<(), StorageError> {
        self.write(move |db| put_session(db, &digest, &record)).await
    }

    /// Renews a live session atomically, without recreating a revoked or expired one.
    /// Concurrent requests can only move its timestamps forward.
    pub async fn renew(&self, digest: [u8; 32], expires_at: Timestamp, last_seen_at: Timestamp) -> Result<bool, StorageError> {
        self.write(move |db| {
            let tx = db.begin_write().map_err(kv_error)?;
            let renewed = {
                let mut table = tx.open_table(SESSIONS).map_err(kv_error)?;
                let record = table.get(&digest).map_err(kv_error)?.and_then(|value| SessionRecord::decode(value.value()));
                if let Some(mut record) = record.filter(|record| !record.is_expired()) {
                    record.expires_at = record.expires_at.max(expires_at);
                    record.last_seen_at = record.last_seen_at.max(last_seen_at);
                    table.insert(&digest, record.encode()?.as_slice()).map_err(kv_error)?;
                    true
                } else {
                    false
                }
            };
            if !renewed {
                tx.abort().map_err(kv_error)?;
                return Ok(false);
            }
            tx.commit().map_err(kv_error)?;
            Ok(true)
        })
        .await
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
                self.write(move |db| {
                    let tx = db.begin_write().map_err(kv_error)?;
                    let expired = tx
                        .open_table(SESSIONS)
                        .map_err(kv_error)?
                        .get(&digest)
                        .map_err(kv_error)?
                        .and_then(|value| SessionRecord::decode(value.value()))
                        .is_none_or(|record| record.is_expired());
                    if expired {
                        remove(&tx, &digest)?;
                    }
                    tx.commit().map_err(kv_error)
                })
                .await?;
                Ok(None)
            }
            other => Ok(other),
        }
    }

    pub async fn delete(&self, digest: [u8; 32]) -> Result<(), StorageError> {
        self.write(move |db| {
            let tx = db.begin_write().map_err(kv_error)?;
            remove(&tx, &digest)?;
            tx.commit().map_err(kv_error)
        })
        .await
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
        self.write(move |db| delete_account_sessions(db, account, keep)).await
    }

    // Only the typed storage coordinator constructs this private ownership bridge.
    pub(crate) async fn finish_forced_password(
        &self,
        account: AccountId,
        mutation: rbit_authority_fence::Mutation,
        command: Arc<crate::sqlite::credential_changes::Command>,
    ) -> Result<usize, StorageError> {
        self.finish_forced_password_with(account, mutation, command, |db, account| delete_account_sessions(db, account, None)).await
    }

    // Monomorphized fault seam; production supplies the fixed revoke-all function.
    pub(crate) async fn finish_forced_password_with<F>(
        &self,
        account: AccountId,
        mutation: rbit_authority_fence::Mutation,
        command: Arc<crate::sqlite::credential_changes::Command>,
        operation: F,
    ) -> Result<usize, StorageError>
    where
        F: FnOnce(&redb::Database, AccountId) -> Result<usize, StorageError> + Send + 'static,
    {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let result = operation(&db, account);
            command.acknowledge(mutation, result.is_ok());
            result
        })
        .await?
    }

    /// Deletes expired sessions from one snapshot/time, rechecking each candidate
    /// in bounded write batches. Read chunks do not hold the authority serial.
    /// Non-success can follow acknowledged earlier batches; it does not imply rollback.
    pub async fn purge_expired(&self) -> Result<usize, StorageError> {
        self.purge_with(PurgeState::scan, PurgeState::flush).await
    }

    // Private static function-item seam: production is monomorphized with the
    // fixed methods above; tests can inject a precisely labeled phase failure.
    async fn purge_with<S, W>(&self, scan: S, write: W) -> Result<usize, StorageError>
    where
        S: Fn(PurgeState) -> Result<PurgeState, StorageError> + Clone + Send + 'static,
        W: Fn(PurgeState, &redb::Database) -> Result<PurgeState, StorageError> + Clone + Send + 'static,
    {
        let reservation = self.purge.clone().try_acquire_owned().map_err(|_| StorageError::Conflict("session purge capacity"))?;
        let db = Arc::clone(&self.db);
        let first_scan = scan.clone();
        let mut state = tokio::task::spawn_blocking(move || first_scan(PurgeState::snapshot(&db, reservation)?)).await??;
        loop {
            if state.candidates.len() == PURGE_BATCH_SIZE || state.done {
                if !state.candidates.is_empty() {
                    let write = write.clone();
                    state = self.write(move |db| write(state, db)).await?;
                }
                if state.done {
                    return Ok(state.removed);
                }
            }
            let scan = scan.clone();
            state = tokio::task::spawn_blocking(move || scan(state)).await??;
        }
    }
}

pub(crate) fn delete_account_sessions(db: &redb::Database, account: AccountId, keep: Option<[u8; 32]>) -> Result<usize, StorageError> {
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
}

fn put_session(db: &redb::Database, digest: &[u8; 32], record: &SessionRecord) -> Result<(), StorageError> {
    let encoded = record.encode()?;
    let tx = db.begin_write().map_err(kv_error)?;
    remove(&tx, digest)?;
    tx.open_table(SESSIONS).map_err(kv_error)?.insert(digest, encoded.as_slice()).map_err(kv_error)?;
    tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.insert((record.account_id.0, digest), ()).map_err(kv_error)?;
    tx.commit().map_err(kv_error)
}

// Range owns the original read snapshot even after its table/transaction handles
// are dropped. Each handoff moves the reservation with actual worker resources.
type SessionRange = redb::Range<'static, &'static [u8; 32], &'static [u8]>;
struct PurgeState {
    range: SessionRange,
    now: Timestamp,
    candidates: Vec<[u8; 32]>,
    removed: usize,
    done: bool,
    _reservation: OwnedSemaphorePermit,
}
impl PurgeState {
    fn snapshot(db: &redb::Database, reservation: OwnedSemaphorePermit) -> Result<Self, StorageError> {
        let now = Timestamp::now();
        let snapshot = db.begin_read().map_err(kv_error)?;
        let table = snapshot.open_table(SESSIONS).map_err(kv_error)?;
        let range = table.range::<&[u8; 32]>(..).map_err(kv_error)?;
        Ok(Self { range, now, candidates: Vec::with_capacity(PURGE_BATCH_SIZE), removed: 0, done: false, _reservation: reservation })
    }

    fn scan(mut self) -> Result<Self, StorageError> {
        for _ in 0..PURGE_BATCH_SIZE {
            if self.candidates.len() == PURGE_BATCH_SIZE {
                break;
            }
            let Some(entry) = self.range.next() else {
                self.done = true;
                break;
            };
            let (key, value) = entry.map_err(kv_error)?;
            if SessionRecord::decode(value.value()).is_none_or(|record| record.expires_at <= self.now) {
                self.candidates.push(*key.value());
            }
        }
        Ok(self)
    }

    fn flush(mut self, db: &redb::Database) -> Result<Self, StorageError> {
        let removed = remove_expired_batch(db, &self.candidates, self.now)?;
        self.removed = self.removed.checked_add(removed).ok_or_else(|| StorageError::Corrupt("session purge count overflow".into()))?;
        self.candidates.clear();
        Ok(self)
    }
}

fn session_mutation_error(error: rbit_authority_fence::BeginError) -> StorageError {
    StorageError::Conflict(match error {
        rbit_authority_fence::BeginError::Capacity => "session mutation capacity",
        rbit_authority_fence::BeginError::Stopped => "session writer stopped",
    })
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
            .is_some_and(|value| SessionRecord::decode(value.value()).is_none_or(|record| record.expires_at <= now));
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
        let store = SessionStore::new(Arc::new(db), rbit_authority_fence::Domain::new().writer(), Arc::new(Semaphore::new(1)));
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
    async fn renewal_never_restores_revoked_or_expired_sessions() {
        let (_dir, store) = store();
        let original = record(7, 3600);
        store.put([1; 32], original.clone()).await.unwrap();
        let stale = store.get([1; 32]).await.unwrap().unwrap();
        store.delete_for_account(AccountId(7), None).await.unwrap();
        assert!(!store.renew([1; 32], stale.expires_at, stale.last_seen_at).await.unwrap());
        assert!(store.get([1; 32]).await.unwrap().is_none());
        assert_eq!(store.list_for_account(AccountId(7)).await.unwrap().len(), 0);

        store.put([2; 32], record(7, -1)).await.unwrap();
        assert!(!store.renew([2; 32], original.expires_at, original.last_seen_at).await.unwrap());
        assert!(store.get([2; 32]).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn renewal_does_not_regress_timestamps() {
        let (_dir, store) = store();
        let original = record(7, 3600);
        store.put([1; 32], original.clone()).await.unwrap();
        let later = Timestamp(original.expires_at.0 + 3600);
        assert!(store.renew([1; 32], later, original.last_seen_at).await.unwrap());
        assert!(store.renew([1; 32], original.expires_at, Timestamp(original.last_seen_at.0 - 10)).await.unwrap());
        let renewed = store.get([1; 32]).await.unwrap().unwrap();
        assert_eq!(renewed.expires_at, later);
        assert_eq!(renewed.last_seen_at, original.last_seen_at);
        assert_eq!(renewed.account_id, original.account_id);
    }

    #[tokio::test]
    async fn replacing_a_session_updates_its_account_index() {
        let (_dir, store) = store();
        store.put([1; 32], record(7, 3600)).await.unwrap();
        store.put([1; 32], record(8, 3600)).await.unwrap();
        assert_eq!(store.list_for_account(AccountId(7)).await.unwrap().len(), 0);
        assert_eq!(store.delete_for_account(AccountId(7), None).await.unwrap(), 0);
        assert_eq!(store.list_for_account(AccountId(8)).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn purges_expired_sessions_and_their_index_entries() {
        let (_dir, store) = store();
        store.put([1; 32], record(1, -1)).await.unwrap();
        store.put([2; 32], record(1, 3600)).await.unwrap();
        assert_eq!(store.purge_expired().await.unwrap(), 1);
        assert_eq!(store.delete_for_account(AccountId(1), None).await.unwrap(), 1, "the purged session left no index entry");
    }

    #[tokio::test]
    async fn scanning_live_sessions_never_waits_for_the_writer() {
        let (_dir, store) = store();
        store.put([1; 32], record(1, 3600)).await.unwrap();
        let writer = store.db.begin_write().unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), store.purge_expired()).await;
        drop(writer);
        assert_eq!(result.expect("a read-only expiry scan must not acquire the write lock").unwrap(), 0);
    }

    #[tokio::test]
    async fn cleanup_rechecks_replaced_and_revoked_candidates() {
        let (_dir, store) = store();
        let now = Timestamp::now();
        store.put([1; 32], record(1, -1)).await.unwrap();
        store.put([2; 32], record(1, -1)).await.unwrap();
        store.put([3; 32], record(1, -1)).await.unwrap();
        // These keys were collected by an earlier read snapshot.
        let candidates = [[1; 32], [2; 32], [3; 32]];
        store.put([1; 32], record(2, 3600)).await.unwrap();
        store.delete([2; 32]).await.unwrap();
        assert_eq!(remove_expired_batch(&store.db, &candidates, now).unwrap(), 1);
        assert_eq!(store.get([1; 32]).await.unwrap().unwrap().account_id, AccountId(2));
        assert_eq!(store.delete_for_account(AccountId(1), None).await.unwrap(), 0);
        assert_eq!(store.delete_for_account(AccountId(2), None).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn purge_spans_multiple_batches_without_missing_keys_or_indexes() {
        let (_dir, store) = store();
        let count = PURGE_BATCH_SIZE * 2 + 1;
        let tx = store.db.begin_write().unwrap();
        {
            let mut sessions = tx.open_table(SESSIONS).unwrap();
            let mut index = tx.open_table(ACCOUNT_SESSIONS).unwrap();
            let expired = record(1, -1).encode().unwrap();
            for id in 0..count {
                let mut digest = [0; 32];
                digest[..8].copy_from_slice(&u64::try_from(id).unwrap().to_be_bytes());
                sessions.insert(&digest, expired.as_slice()).unwrap();
                index.insert((1, &digest), ()).unwrap();
            }
        }
        tx.commit().unwrap();
        store.put([255; 32], record(1, 3600)).await.unwrap();
        assert_eq!(store.purge_expired().await.unwrap(), count);
        assert_eq!(store.purge_expired().await.unwrap(), 0);
        assert_eq!(store.delete_for_account(AccountId(1), None).await.unwrap(), 1);
    }
}

#[cfg(test)]
mod worker_tests;
