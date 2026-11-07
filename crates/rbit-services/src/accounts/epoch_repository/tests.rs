use super::*;
use crate::Services;
use rbit_core::{Config, Timestamp};
use rbit_storage::{Database, KvStore};
use std::{sync::Arc, task::Poll};

#[tokio::test]
async fn cancelled_waiter_cannot_escape_close_or_abandon_the_actual_store_write() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().to_path_buf();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let accounts = services.accounts.clone();
    let (started, start) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    let request = tokio::spawn(async move {
        OwnedEpochSessions::new(&accounts)
            .run(move |store| async move {
                started.send(()).unwrap();
                resume.await.unwrap();
                let now = Timestamp::now();
                let record = SessionRecord::at(AccountId(7), Timestamp(now.0 + 3600), None, "owned fixture", now);
                store.put_bound([1; 32], record, AccountId(7), CredentialEpoch::new(1).unwrap()).await
            })
            .await
    });
    start.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    let mut shutdown = Box::pin(services.accounts.shutdown_session_effects());
    let first = std::future::poll_fn(|cx| Poll::Ready(shutdown.as_mut().poll(cx))).await;
    assert!(first.is_pending());
    let report = services.accounts.session_effect_report();
    assert!(report.closed);
    // One admitted context token and one tracked worker retain the same operation.
    assert_eq!(report.active_owners, 2);
    release.send(()).unwrap();
    shutdown.await;
    let report = services.accounts.session_effect_report();
    assert!(report.drained());
    assert_eq!(report.completed, 1);
    assert_eq!(report.abnormal, 0);
    assert!(kv.sessions().epoch_sidecar().get_snapshot([1; 32]).await.unwrap().is_some());
}
