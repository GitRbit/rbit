//! Owned maintenance finishes a started store operation before acknowledging shutdown.

use std::time::Duration;

use rbit_kernel::TaskLifecycle;
use rbit_storage::KvStore;

const SESSION_PURGE_INTERVAL: Duration = Duration::from_hours(1);

pub(super) fn spawn_session_purger(kv: KvStore) -> TaskLifecycle {
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
            match kv.sessions().purge_expired().await {
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
    use super::*;
    use rbit_kernel::TaskState;

    #[tokio::test]
    async fn stopping_releases_the_store_before_maintenance_owner_returns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.redb");
        let store = KvStore::open(&path).unwrap();
        let owner = spawn_session_purger(store);
        owner.shutdown().await;
        assert_eq!(owner.state(), TaskState::Stopped);
        KvStore::open(&path).unwrap();
    }
}
