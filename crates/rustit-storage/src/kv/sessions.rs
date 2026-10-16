//! Login sessions keyed by the SHA-256 digest of the session token.
//!
//! Storing only the digest means a copy of the database cannot be replayed as cookies.

use std::sync::Arc;

use redb::{ReadableDatabase, ReadableTableMetadata, TableDefinition};
use rustit_core::{AccountId, Timestamp};

use crate::{StorageError, error::kv_error};

/// `sha256(token)` → encoded [`SessionRecord`].
const SESSIONS: TableDefinition<'_, &[u8; 32], &[u8; SessionRecord::ENCODED_LEN]> = TableDefinition::new("sessions");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRecord {
    pub account_id: AccountId,
    pub expires_at: Timestamp,
}

impl SessionRecord {
    const ENCODED_LEN: usize = 16;

    /// Fixed-width little-endian encoding: `account_id (i64) ‖ expires_at (i64)`.
    fn encode(self) -> [u8; Self::ENCODED_LEN] {
        let mut bytes = [0; Self::ENCODED_LEN];
        bytes[..8].copy_from_slice(&self.account_id.0.to_le_bytes());
        bytes[8..].copy_from_slice(&self.expires_at.0.to_le_bytes());
        bytes
    }

    fn decode(bytes: &[u8; Self::ENCODED_LEN]) -> Self {
        let (id, expires) = bytes.split_at(8);
        Self {
            account_id: AccountId(i64::from_le_bytes(id.try_into().expect("8 bytes"))),
            expires_at: Timestamp(i64::from_le_bytes(expires.try_into().expect("8 bytes"))),
        }
    }

    pub fn is_expired(self) -> bool {
        self.expires_at <= Timestamp::now()
    }
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    db: Arc<redb::Database>,
}

impl SessionStore {
    pub(crate) fn new(db: Arc<redb::Database>) -> Self {
        Self { db }
    }

    /// Creates the table so read transactions never fail on a fresh database.
    pub(crate) fn ensure_table(&self) -> Result<(), StorageError> {
        let tx = self.db.begin_write().map_err(kv_error)?;
        tx.open_table(SESSIONS).map_err(kv_error)?;
        tx.commit().map_err(kv_error)
    }

    pub async fn put(&self, digest: [u8; 32], record: SessionRecord) -> Result<(), StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            tx.open_table(SESSIONS).map_err(kv_error)?.insert(&digest, &record.encode()).map_err(kv_error)?;
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
            Ok(table.get(&digest).map_err(kv_error)?.map(|value| SessionRecord::decode(value.value())))
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
            tx.open_table(SESSIONS).map_err(kv_error)?.remove(&digest).map_err(kv_error)?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }

    /// Deletes every expired session. Intended for a periodic background task.
    pub async fn purge_expired(&self) -> Result<usize, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let now = Timestamp::now();
            let tx = db.begin_write().map_err(kv_error)?;
            let removed = {
                let mut table = tx.open_table(SESSIONS).map_err(kv_error)?;
                let before = table.len().map_err(kv_error)?;
                table.retain(|_, value| SessionRecord::decode(value).expires_at > now).map_err(kv_error)?;
                before - table.len().map_err(kv_error)?
            };
            tx.commit().map_err(kv_error)?;
            Ok(usize::try_from(removed).unwrap_or(usize::MAX))
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trips() {
        let record = SessionRecord { account_id: AccountId(42), expires_at: Timestamp(1_900_000_000) };
        assert_eq!(SessionRecord::decode(&record.encode()), record);
    }
}
