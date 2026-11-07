//! Complete bounded-memory session/index/binding inspection, including deleted-account orphans.
use super::super::super::LEGACY_SESSIONS;
use super::super::{ACCOUNT_SESSIONS, Binding, EPOCHS, SESSIONS, decode_session};
use crate::{
    error::kv_error,
    identity_upgrade::model::{IdentityUpgradeError as Error, IdentityUpgradeLimits},
};
use redb::{MultimapTableHandle, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableHandle};
#[derive(Default)]
pub(crate) struct Scan {
    pub floor: i64,
    pub pair: Option<[u8; 16]>,
    pub checkpoint: Option<crate::identity_upgrade::model::Checkpoint>,
    entries: u64,
}
impl Scan {
    fn visit(&mut self, limits: IdentityUpgradeLimits) -> Result<(), Error> {
        self.entries = self.entries.checked_add(1).ok_or(Error::IncompleteScan)?;
        if self.entries > limits.max_entries.get() { Err(Error::IncompleteScan) } else { Ok(()) }
    }
}
pub(super) fn inspect(db: &redb::Database, limits: IdentityUpgradeLimits) -> Result<Scan, Error> {
    inspect_layout(db, limits, false)
}
#[cfg(test)]
pub(super) fn inspect_active(db: &redb::Database, limits: IdentityUpgradeLimits) -> Result<Scan, Error> {
    inspect_layout(db, limits, true)
}
fn inspect_layout(db: &redb::Database, limits: IdentityUpgradeLimits, active: bool) -> Result<Scan, Error> {
    let tx = db.begin_read().map_err(kv_error)?;
    let mut scan = Scan { checkpoint: super::mode(db)?, ..Scan::default() };
    for table in tx.list_tables().map_err(kv_error)? {
        scan.visit(limits)?;
        let name = table.name();
        if (name.starts_with("session") || name.starts_with("account_sessions") || name.starts_with("credential_session"))
            && ![
                SESSIONS.name(),
                ACCOUNT_SESSIONS.name(),
                EPOCHS.name(),
                LEGACY_SESSIONS.name(),
                super::MODE.name(),
                super::super::identity::PAIR.name(),
            ]
            .contains(&name)
            && !(active && ["credential_session_active_v3", "credential_session_floor_v3"].contains(&name))
        {
            return Err(Error::Invalid);
        }
    }
    for table in tx.list_multimap_tables().map_err(kv_error)? {
        scan.visit(limits)?;
        let name = table.name();
        if name.starts_with("session") || name.starts_with("account_sessions") || name.starts_with("credential_session") {
            return Err(Error::Invalid);
        }
    }
    macro_rules! optional {
        ($table:expr) => {
            match tx.open_table($table) {
                Ok(t) => Some(t),
                Err(redb::TableError::TableDoesNotExist(_)) => None,
                Err(e) => return Err(kv_error(e).into()),
            }
        };
    }
    let records = optional!(SESSIONS);
    let index = optional!(ACCOUNT_SESSIONS);
    let epochs = optional!(EPOCHS);
    if let Some(records) = &records {
        for entry in records.iter().map_err(kv_error)? {
            scan.visit(limits)?;
            let (digest, value) = entry.map_err(kv_error)?;
            if value.value().len() > limits.max_record_bytes.get() as usize {
                return Err(Error::IncompleteScan);
            }
            let record = decode_session(value.value()).ok_or(Error::Invalid)?;
            if record.account_id.0 <= 0 {
                return Err(Error::Invalid);
            }
            if let Some(epochs) = &epochs
                && let Some(binding) = epochs.get(digest.value()).map_err(kv_error)?
            {
                let binding = Binding::decode(binding.value(), record.account_id)?;
                scan.floor = scan.floor.max(binding.epoch.get());
            }
        }
    }
    if let Some(index) = &index {
        inspect_index(index, records.as_ref(), &mut scan, limits)?;
    }
    if let Some(epochs) = &epochs {
        for entry in epochs.iter().map_err(kv_error)? {
            scan.visit(limits)?;
            let (_, value) = entry.map_err(kv_error)?;
            let bytes = value.value();
            let account = i64::from_be_bytes(bytes[..8].try_into().map_err(|_| Error::Invalid)?);
            if account <= 0 {
                return Err(Error::Invalid);
            }
            let binding = Binding::decode(bytes, rbit_core::AccountId(account))?;
            scan.floor = scan.floor.max(binding.epoch.get());
        }
    }
    if let Some(legacy) = optional!(LEGACY_SESSIONS) {
        for entry in legacy.iter().map_err(kv_error)? {
            scan.visit(limits)?;
            let (_, value) = entry.map_err(kv_error)?;
            let account = i64::from_be_bytes(value.value()[..8].try_into().map_err(|_| Error::Invalid)?);
            if account <= 0 {
                return Err(Error::Invalid);
            }
        }
    }
    if let Some(pair) = optional!(super::super::identity::PAIR) {
        if pair.len().map_err(kv_error)? != 1 {
            return Err(Error::Invalid);
        }
        scan.pair = Some(*pair.get(0).map_err(kv_error)?.ok_or(Error::Invalid)?.value());
    }
    if scan.floor == i64::MAX {
        return Err(Error::Exhausted);
    }
    Ok(scan)
}
/// Every index entry names a positive account and agrees with any record it points to.
fn inspect_index(
    index: &redb::ReadOnlyTable<(i64, &'static [u8; 32]), ()>,
    records: Option<&redb::ReadOnlyTable<&'static [u8; 32], &'static [u8]>>,
    scan: &mut Scan,
    limits: IdentityUpgradeLimits,
) -> Result<(), Error> {
    for entry in index.iter().map_err(kv_error)? {
        scan.visit(limits)?;
        let (key, _) = entry.map_err(kv_error)?;
        let (account, digest) = key.value();
        if account <= 0 {
            return Err(Error::Invalid);
        }
        if let Some(records) = records
            && let Some(value) = records.get(digest).map_err(kv_error)?
        {
            if value.value().len() > limits.max_record_bytes.get() as usize {
                return Err(Error::IncompleteScan);
            }
            if decode_session(value.value()).ok_or(Error::Invalid)?.account_id.0 != account {
                return Err(Error::Invalid);
            }
        }
    }
    Ok(())
}
