//! Existing-file stopped inspection/cutover. No ordinary session reader calls this.
mod scan;
use super::super::LEGACY_SESSIONS;
use super::{ACCOUNT_SESSIONS, EPOCHS, SESSIONS};
use crate::{
    error::kv_error,
    identity_upgrade::model::{Checkpoint, IdentityUpgradeError as Error, IdentityUpgradeLimits},
};
use redb::{MultimapTableHandle, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition, TableHandle};
pub(crate) use scan::Scan;
const MODE: TableDefinition<'_, u8, &[u8; 64]> = TableDefinition::new("credential_session_protocol_v2");
pub(crate) fn mode(db: &redb::Database) -> Result<Option<Checkpoint>, Error> {
    let tx = db.begin_read().map_err(kv_error)?;
    let table = match tx.open_table(MODE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(kv_error(e).into()),
    };
    if table.len().map_err(kv_error)? != 1 {
        return Err(Error::Invalid);
    }
    let value = table.get(0).map_err(kv_error)?.ok_or(Error::Invalid)?;
    Ok(Some(Checkpoint::decode(value.value())?))
}
pub(crate) fn inspect(db: &redb::Database, limits: IdentityUpgradeLimits) -> Result<Scan, Error> {
    scan::inspect(db, limits)
}
pub(crate) fn prepare(db: &redb::Database, c: Checkpoint, limits: IdentityUpgradeLimits) -> Result<(), Error> {
    if let Some(existing) = mode(db)? {
        if existing != c {
            return Err(Error::Mismatch);
        }
        return verify_empty(db, c, limits);
    }
    let scan = inspect(db, limits)?;
    if scan.floor > c.floor || scan.pair.is_some_and(|pair| pair != c.pair) {
        return Err(Error::Mismatch);
    }
    let mut tx = db.begin_write().map_err(kv_error)?;
    tx.set_durability(redb::Durability::Immediate).map_err(kv_error)?;
    // This raw owner has the exclusive default backend handle; no shared SessionStore
    // is ever issued. Successful inspection cannot race an official KV writer.
    for name in [SESSIONS.name(), ACCOUNT_SESSIONS.name(), EPOCHS.name(), LEGACY_SESSIONS.name()] {
        if tx.list_tables().map_err(kv_error)?.any(|table| table.name() == name) {
            // delete_table uses only the handle name; inspection already checked its schema.
            tx.delete_table(redb::TableDefinition::<(), ()>::new(name)).map_err(kv_error)?;
        }
    }
    tx.open_table(SESSIONS).map_err(kv_error)?;
    tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?;
    tx.open_table(EPOCHS).map_err(kv_error)?;
    let mut pair = tx.open_table(super::identity::PAIR).map_err(kv_error)?;
    if pair.len().map_err(kv_error)? == 0 {
        pair.insert(0, &c.pair).map_err(kv_error)?;
    } else if pair.get(0).map_err(kv_error)?.is_none_or(|value| value.value() != &c.pair) {
        return Err(Error::Mismatch);
    }
    drop(pair);
    tx.open_table(MODE).map_err(kv_error)?.insert(0, &c.encode()).map_err(kv_error)?;
    tx.commit().map_err(kv_error)?;
    Ok(())
}
pub(crate) fn verify_empty(db: &redb::Database, c: Checkpoint, limits: IdentityUpgradeLimits) -> Result<(), Error> {
    if mode(db)? != Some(c) {
        return Err(Error::Mismatch);
    }
    let scan = inspect(db, limits)?;
    if scan.floor != 0 || scan.pair != Some(c.pair) {
        return Err(Error::Mismatch);
    }
    let tx = db.begin_read().map_err(kv_error)?;
    if tx.open_table(SESSIONS).map_err(kv_error)?.len().map_err(kv_error)? != 0
        || tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?.len().map_err(kv_error)? != 0
        || tx.open_table(EPOCHS).map_err(kv_error)?.len().map_err(kv_error)? != 0
    {
        return Err(Error::Mismatch);
    }
    if tx.list_tables().map_err(kv_error)?.any(|table| table.name() == LEGACY_SESSIONS.name()) {
        return Err(Error::Mismatch);
    }
    let pair = tx.open_table(super::identity::PAIR).map_err(kv_error)?;
    if pair.len().map_err(kv_error)? != 1 || pair.get(0).map_err(kv_error)?.is_none_or(|value| value.value() != &c.pair) {
        return Err(Error::Mismatch);
    }
    Ok(())
}

pub(crate) fn legacy_allowed(db: &redb::Database) -> Result<(), crate::StorageError> {
    if mode(db).map_err(|error| crate::StorageError::Corrupt(error.to_string()))?.is_some() {
        return Err(crate::StorageError::Corrupt(
            "prepared identity stores require the complete activation bundle, not a legacy open".into(),
        ));
    }
    let tx = db.begin_read().map_err(kv_error)?;
    for table in tx.list_tables().map_err(kv_error)? {
        if table.name().starts_with("credential_session_protocol") {
            return Err(crate::StorageError::Corrupt("unknown identity protocol".into()));
        }
    }
    for table in tx.list_multimap_tables().map_err(kv_error)? {
        if table.name().starts_with("credential_session_protocol") {
            return Err(crate::StorageError::Corrupt("unknown identity protocol".into()));
        }
    }
    Ok(())
}
