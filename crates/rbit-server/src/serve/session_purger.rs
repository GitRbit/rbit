//! Owned maintenance finishes a started store operation before acknowledging shutdown.

use std::time::Duration;

use rbit_kernel::TaskLifecycle;
use rbit_services::AccountService;

const SESSION_PURGE_INTERVAL: Duration = Duration::from_hours(1);

/// Expired-session cleanup runs through the identity session application.
pub(super) fn spawn_session_purger(accounts: AccountService) -> TaskLifecycle {
    let owner = TaskLifecycle::new();
    let stop = owner.stop_signal();
    owner.start(async move {
        let mut interval = tokio::time::interval(SESSION_PURGE_INTERVAL);
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => break,
                _ = interval.tick() => {},
            }
            // The store may own blocking work; cancelling its waiter is not proof
            // that work stopped. Finish this purge before observing cancellation.
            match accounts.purge_expired_sessions().await {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "purged expired sessions"),
                Err(error) => tracing::warn!(%error, "session purge failed"),
            }
        }
    });
    owner
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use rbit_core::Config;
    use rbit_kernel::TaskState;
    use rbit_services::Services;
    use rbit_storage::{Database, KvStore};

    #[tokio::test]
    async fn stopping_releases_the_store_before_maintenance_owner_returns() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let path = config.storage.kv_path();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let services = Services::new(Arc::new(config), &database, &KvStore::open(&path).unwrap()).await.unwrap();
        let owner = spawn_session_purger(services.accounts.clone());
        drop(services);
        owner.shutdown().await;
        assert_eq!(owner.state(), TaskState::Stopped);
        database.close().await;
        KvStore::open(&path).unwrap();
    }
}
