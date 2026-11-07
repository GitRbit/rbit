//! The native owner is retained throughout each actual blocking transaction.
use super::super::{IdentityUpgradeError as Error, IdentityUpgradeLimits, model::Checkpoint};
use crate::kv::sessions::epoch::runtime;
use crate::{error::kv_error, kv::sessions::epoch::stopped};
use redb::{ReadableDatabase, ReadableTableMetadata};
use runtime::ACTIVE;
pub(super) fn verify_prepared(db: &redb::Database, checkpoint: Checkpoint, limits: IdentityUpgradeLimits) -> Result<(), Error> {
    if is_active(db, checkpoint, limits)? {
        return Err(Error::Mismatch);
    }
    stopped::verify_empty(db, checkpoint, limits)
}
pub(super) fn is_active(db: &redb::Database, checkpoint: Checkpoint, limits: IdentityUpgradeLimits) -> Result<bool, Error> {
    stopped::active_layout(db, limits)?;
    if stopped::mode(db)? != Some(checkpoint) {
        return Err(Error::Mismatch);
    }
    let tx = db.begin_read().map_err(kv_error)?;
    let table = match tx.open_table(ACTIVE) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => return if runtime::read(db)?.is_none() { Ok(false) } else { Err(Error::Mismatch) },
        Err(error) => return Err(kv_error(error).into()),
    };
    if table.len().map_err(kv_error)? != 1 || table.get(0).map_err(kv_error)?.is_none_or(|value| value.value() != &checkpoint.encode()) {
        return Err(Error::Mismatch);
    }
    if !stopped::matches_pair(db, checkpoint.pair)? {
        return Err(Error::Mismatch);
    }
    let initial_floor = checkpoint.final_floor()?;
    if runtime::read(db)?.is_none_or(|floor| floor < initial_floor) {
        return Err(Error::Mismatch);
    }
    Ok(true)
}
pub(super) fn publish(db: &redb::Database, checkpoint: Checkpoint, limits: IdentityUpgradeLimits) -> Result<(), Error> {
    if is_active(db, checkpoint, limits)? {
        return Ok(());
    }
    stopped::verify_empty(db, checkpoint, limits)?;
    let mut tx = db.begin_write().map_err(kv_error)?;
    tx.set_durability(redb::Durability::Immediate).map_err(kv_error)?;
    tx.open_table(ACTIVE).map_err(kv_error)?.insert(0, &checkpoint.encode()).map_err(kv_error)?;
    runtime::initialize_in(&tx, checkpoint.final_floor()?)?;
    tx.commit().map_err(kv_error)?;
    Ok(())
}
