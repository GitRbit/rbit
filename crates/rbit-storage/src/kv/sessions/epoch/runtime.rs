//! Mandatory ACTIVE runtime witness. Actual session/finalizer transactions must call
//! `advance_in` before acknowledgment; this gated foundation does not wire them yet.
use crate::{error::kv_error, identity_upgrade::IdentityUpgradeError as Error};
use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
pub(crate) const FLOOR: TableDefinition<'_, u8, i64> = TableDefinition::new("credential_session_floor_v3");
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
