//! Explicit empty-store identity initialization. No open/migration automatically calls it.
use super::super::LEGACY_SESSIONS;
use super::{ACCOUNT_SESSIONS, EPOCHS, EpochSessionStore, SESSIONS, corrupt};
use crate::{StorageError, error::kv_error};
use rbit_identity_models::sessions::SessionPairIdentity;
use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition, TableHandle};
use std::sync::Arc;
pub(super) const PAIR: TableDefinition<'_, u8, &[u8; 16]> = TableDefinition::new("credential_session_identity_v1");
/// Owns only pair metadata; ordinary session lifecycle operations remain separate.
pub(super) struct PairIdentityStore {
    db: Arc<redb::Database>,
    random: fn(&mut [u8]) -> Result<(), ()>,
}
impl EpochSessionStore {
    fn identity_store(&self) -> PairIdentityStore {
        PairIdentityStore { db: Arc::clone(&self.db), random: self.random }
    }
    pub async fn pair_identity(&self) -> Result<Option<SessionPairIdentity>, StorageError> {
        self.identity_store().pair_identity().await
    }
    pub async fn initialize_identity_if_empty(&self) -> Result<SessionPairIdentity, StorageError> {
        #[cfg(test)]
        self.mode.require_legacy()?;
        self.identity_store().initialize_identity_if_empty().await
    }
}
impl PairIdentityStore {
    pub async fn pair_identity(&self) -> Result<Option<SessionPairIdentity>, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            let table = match tx.open_table(PAIR) {
                Ok(table) => table,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
                Err(error) => return Err(kv_error(error)),
            };
            if table.len().map_err(kv_error)? != 1 {
                return Err(corrupt());
            }
            let value = table.get(0).map_err(kv_error)?.ok_or_else(corrupt)?;
            SessionPairIdentity::from_storage(value.value()).map(Some).ok_or_else(corrupt)
        })
        .await?
    }
    /// Explicit prototype/upgrade seam only. Refuses any existing session/index/
    /// sidecar/legacy record. It never adopts, purges or changes an existing identity.
    /// Full startup acceptance, stopped-writer proof and SQL binding remain separate.
    pub async fn initialize_identity_if_empty(&self) -> Result<SessionPairIdentity, StorageError> {
        if let Some(identity) = self.pair_identity().await? {
            return Ok(identity);
        }
        let mut bytes = [0_u8; 16];
        (self.random)(&mut bytes).map_err(|()| corrupt())?;
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            if tx.list_tables().map_err(kv_error)?.any(|table| table.name() == PAIR.name()) {
                return in_write(&tx);
            }
            if tx.open_table(SESSIONS).map_err(kv_error)?.len().map_err(kv_error)? != 0
                || tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.len().map_err(kv_error)? != 0
                || tx.open_table(EPOCHS).map_err(kv_error)?.len().map_err(kv_error)? != 0
            {
                return Err(corrupt());
            }
            if tx.list_tables().map_err(kv_error)?.any(|table| table.name() == LEGACY_SESSIONS.name())
                && tx.open_table(LEGACY_SESSIONS).map_err(kv_error)?.len().map_err(kv_error)? != 0
            {
                return Err(corrupt());
            }
            tx.open_table(PAIR).map_err(kv_error)?.insert(0, &bytes).map_err(kv_error)?;
            tx.commit().map_err(kv_error)?;
            SessionPairIdentity::from_storage(&bytes).ok_or_else(corrupt)
        })
        .await?
    }
}
pub(super) fn in_write(tx: &redb::WriteTransaction) -> Result<SessionPairIdentity, StorageError> {
    let table = tx.open_table(PAIR).map_err(kv_error)?;
    if table.len().map_err(kv_error)? != 1 {
        return Err(corrupt());
    }
    let value = table.get(0).map_err(kv_error)?.ok_or_else(corrupt)?;
    SessionPairIdentity::from_storage(value.value()).ok_or_else(corrupt)
}
