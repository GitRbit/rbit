//! Read-only batch projection; retains the original index walk, allocation and sort.
use super::{ACCOUNT_SESSIONS, Binding, EPOCHS, EpochSnapshot, SESSIONS, corrupt, decode_session};
use crate::{StorageError, error::kv_error};
use rbit_core::AccountId;
use redb::ReadableDatabase;
use std::sync::Arc;
pub(super) struct BoundSessionListing {
    db: Arc<redb::Database>,
}
impl BoundSessionListing {
    pub fn new(db: Arc<redb::Database>) -> Self {
        Self { db }
    }
    pub async fn list(self, account: AccountId) -> Result<Vec<EpochSnapshot>, StorageError> {
        let db = self.db;
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            let records = tx.open_table(SESSIONS).map_err(kv_error)?;
            let index = tx.open_table(ACCOUNT_SESSIONS).map_err(kv_error)?;
            let epochs = match tx.open_table(EPOCHS) {
                Ok(table) => Some(table),
                Err(redb::TableError::TableDoesNotExist(_)) => None,
                Err(error) => return Err(kv_error(error)),
            };
            let mut snapshots = Vec::new();
            for entry in index.range((account.0, &[0_u8; 32])..=(account.0, &[0xff_u8; 32])).map_err(kv_error)? {
                let (key, _) = entry.map_err(kv_error)?;
                let digest = *key.value().1;
                let record = records.get(&digest).map_err(kv_error)?.and_then(|value| decode_session(value.value())).ok_or_else(corrupt)?;
                if record.account_id != account {
                    return Err(corrupt());
                }
                let binding = epochs.as_ref().ok_or_else(corrupt)?.get(&digest).map_err(kv_error)?.ok_or_else(corrupt)?;
                let binding = Binding::decode(binding.value(), account)?;
                if !record.is_expired() {
                    snapshots.push(EpochSnapshot { digest, record, binding: Some(binding) });
                }
            }
            snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.record.last_seen_at));
            Ok(snapshots)
        })
        .await?
    }
}
