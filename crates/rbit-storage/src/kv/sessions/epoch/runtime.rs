//! Mandatory ACTIVE runtime witness. Actual session/finalizer transactions must call
//! `advance_in` before acknowledgment. Store selection remains test-gated.
use crate::{error::kv_error, identity_upgrade::IdentityUpgradeError as Error};
use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
pub(crate) const FLOOR: TableDefinition<'_, u8, i64> = TableDefinition::new("credential_session_floor_v3");
pub(crate) const ACTIVE: TableDefinition<'_, u8, &[u8; 64]> = TableDefinition::new("credential_session_active_v3");
use crate::{
    StorageError,
    identity_upgrade::{active::ActiveIdentityPair, model::Checkpoint},
};
use rbit_identity_models::credential::CredentialEpoch;

/// Private in-memory composition fact. No public bool, parsed-marker constructor,
/// Serde or binding/debug disclosure. Production selection remains gated.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionMode {
    checkpoint: Option<Checkpoint>,
}
impl std::fmt::Debug for SessionMode {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SessionMode").finish_non_exhaustive()
    }
}
impl SessionMode {
    pub(crate) const fn legacy() -> Self {
        Self { checkpoint: None }
    }
    pub(crate) fn require_legacy(self) -> Result<(), StorageError> {
        if self.checkpoint.is_some() {
            return Err(refused());
        }
        Ok(())
    }
    pub(crate) fn requires_binding(self) -> bool {
        self.checkpoint.is_some()
    }
    /// Same write transaction checks immutable publications and pair; no extra TX,
    /// SQL lookup or scan. Legacy mode performs no metadata operation.
    pub(crate) fn prepare(self, mut tx: redb::WriteTransaction) -> Result<redb::WriteTransaction, StorageError> {
        if let Some(checkpoint) = self.checkpoint {
            for definition in [ACTIVE, super::stopped::MODE] {
                let table = tx.open_table(definition).map_err(kv_error)?;
                if table.len().map_err(kv_error)? != 1
                    || table.get(0).map_err(kv_error)?.is_none_or(|value| value.value() != &checkpoint.encode())
                {
                    return Err(refused());
                }
            }
            let pair = super::identity::in_write(&tx)?;
            if pair.storage_bytes() != checkpoint.pair {
                return Err(refused());
            }
            let table = tx.open_table(FLOOR).map_err(kv_error)?;
            if checked(&table).map_err(|error| storage(&error))? < checkpoint.final_floor().map_err(|error| storage(&error))? {
                return Err(refused());
            }
            drop(table);
            tx.set_durability(redb::Durability::Immediate).map_err(kv_error)?;
        }
        Ok(tx)
    }
    pub(crate) fn advance(self, tx: &redb::WriteTransaction, epoch: CredentialEpoch) -> Result<(), StorageError> {
        if self.checkpoint.is_some() {
            advance_in(tx, epoch.get()).map_err(|error| storage(&error))?;
        }
        Ok(())
    }
}
pub(crate) fn sessions(pair: &ActiveIdentityPair) -> super::super::SessionStore {
    let (db, checkpoint) = pair.runtime_parts();
    super::super::SessionStore { db: std::sync::Arc::clone(db), mode: SessionMode { checkpoint: Some(checkpoint) } }
}
fn refused() -> StorageError {
    StorageError::Corrupt("active identity sessions require the captured fenced protocol".into())
}
fn storage(error: &Error) -> StorageError {
    StorageError::Corrupt(error.to_string())
}
pub(crate) fn read(db: &redb::Database) -> Result<Option<i64>, Error> {
    let tx = db.begin_read().map_err(kv_error)?;
    let table = match tx.open_table(FLOOR) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(error) => return Err(kv_error(error).into()),
    };
    checked(&table).map(Some)
}
fn checked(table: &impl ReadableTable<u8, i64>) -> Result<i64, Error> {
    if table.len().map_err(kv_error)? != 1 {
        return Err(Error::Invalid);
    }
    let value = table.get(0).map_err(kv_error)?.ok_or(Error::Invalid)?.value();
    if !(0..i64::MAX).contains(&value) {
        return Err(Error::Invalid);
    }
    Ok(value)
}
/// Only initial publication may create the floor, in the SAME durable transaction
/// as ACTIVE. Runtime mutations must not recreate/adopt missing witness state.
pub(crate) fn initialize_in(tx: &redb::WriteTransaction, floor: i64) -> Result<(), Error> {
    if !(0..i64::MAX).contains(&floor) {
        return Err(Error::Exhausted);
    }
    let mut table = tx.open_table(FLOOR).map_err(kv_error)?;
    if table.len().map_err(kv_error)? != 0 {
        return Err(Error::Mismatch);
    }
    table.insert(0, floor).map_err(kv_error)?;
    Ok(())
}
/// An out-of-order captured mutation may retain a newer floor, never lower it.
/// A transaction error/rollback must also roll back this witness advancement.
pub(crate) fn advance_in(tx: &redb::WriteTransaction, target: i64) -> Result<(), Error> {
    if !(1..i64::MAX).contains(&target) {
        return Err(Error::Exhausted);
    }
    let mut table = tx.open_table(FLOOR).map_err(kv_error)?;
    let current = checked(&table)?;
    if target > current {
        table.insert(0, target).map_err(kv_error)?;
    }
    Ok(())
}
