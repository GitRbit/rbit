//! Import record admission and restart cleanup share the same lifecycle gate.

use super::{INTERRUPTED, ImportService};
use rbit_core::{RepoId, Timestamp};
use rbit_storage::{StorageError, sqlite::imports::NewImport};

impl ImportService {
    pub(super) async fn create_record(&self, record: &NewImport<'_>) -> Result<(), StorageError> {
        let mut running = self.inner.running.lock().await;
        running.insert(record.repo);
        if let Err(error) = self.store().create(record).await {
            running.remove(&record.repo);
            return Err(error);
        }
        // The gate ends before spawning or awaiting any Git/network import work.
        Ok(())
    }

    pub(super) async fn fail_interrupted(&self) -> Result<u64, StorageError> {
        let running = self.inner.running.lock().await;
        let excluded: Vec<RepoId> = running.iter().copied().collect();
        // A snapshot alone is insufficient: a new queued row must not appear
        // between this snapshot and the UPDATE that declares it interrupted.
        let result = self.store().fail_unfinished(Timestamp::now(), INTERRUPTED, &excluded).await;
        drop(running);
        result
    }
}

#[cfg(test)]
mod tests;
