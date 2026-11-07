//! Consuming write role: exactly one existing backend handoff per operation.
use super::{Binding, EPOCHS, EpochSnapshot, SESSIONS, SessionRecord, encode_session, insert, matched, remove};
use crate::{StorageError, error::kv_error};
use rbit_core::Timestamp;
use std::sync::Arc;
pub(super) struct BoundSessionMutations {
    pub db: Arc<redb::Database>,
    #[cfg(test)]
    pub mode: super::runtime::SessionMode,
}
impl BoundSessionMutations {
    pub fn for_store(store: &super::EpochSessionStore) -> Self {
        Self {
            db: Arc::clone(&store.db),
            #[cfg(test)]
            mode: store.mode,
        }
    }
    pub async fn put_bound(
        self,
        digest: [u8; 32],
        record: SessionRecord,
        encoded: Vec<u8>,
        binding: Binding,
    ) -> Result<Option<EpochSnapshot>, StorageError> {
        #[cfg(test)]
        let mode = self.mode;
        let db = self.db;
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            #[cfg(test)]
            let tx = mode.prepare(tx)?;
            if !insert(&tx, digest, &record, &encoded, binding)? {
                return Ok(None);
            }
            #[cfg(test)]
            mode.advance(&tx, binding.epoch)?;
            tx.commit().map_err(kv_error)?;
            Ok(Some(EpochSnapshot { digest, record, binding: Some(binding) }))
        })
        .await?
    }

    pub async fn renew_captured(
        self,
        expected: EpochSnapshot,
        binding: Binding,
        expires: Timestamp,
        seen: Timestamp,
    ) -> Result<bool, StorageError> {
        #[cfg(test)]
        let mode = self.mode;
        let db = self.db;
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            #[cfg(test)]
            let tx = mode.prepare(tx)?;
            let Some(mut current) = matched(&tx, &expected, binding)? else { return Ok(false) };
            if current.is_expired() {
                return Ok(false);
            }
            current.expires_at = current.expires_at.max(expires);
            current.last_seen_at = current.last_seen_at.max(seen);
            tx.open_table(SESSIONS).map_err(kv_error)?.insert(&expected.digest, encode_session(&current)?.as_slice()).map_err(kv_error)?;
            #[cfg(test)]
            mode.advance(&tx, binding.epoch)?;
            tx.commit().map_err(kv_error)?;
            Ok(true)
        })
        .await?
    }

    pub async fn delete_captured(self, expected: EpochSnapshot, binding: Binding) -> Result<bool, StorageError> {
        #[cfg(test)]
        let mode = self.mode;
        let db = self.db;
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            #[cfg(test)]
            let tx = mode.prepare(tx)?;
            if matched(&tx, &expected, binding)?.is_none() {
                return Ok(false);
            }
            remove(&tx, &expected.digest)?;
            tx.open_table(EPOCHS).map_err(kv_error)?.remove(&expected.digest).map_err(kv_error)?;
            #[cfg(test)]
            mode.advance(&tx, binding.epoch)?;
            tx.commit().map_err(kv_error)?;
            Ok(true)
        })
        .await?
    }
}
