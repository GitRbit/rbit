//! SQL and actual session methods share construction binding and terminal admission.
use super::*;
use crate::{NewUser, kv::SessionRecord};
use rbit_core::{AccountId, Timestamp};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::Instant;

async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
#[tokio::test]
async fn sql_and_kv_session_handles_share_serial_and_terminal_stop() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let clone = database.clone();
    let kv = clone.open_kv(&dir.path().join("kv.redb")).unwrap();
    let sessions = kv.sessions();
    let record = SessionRecord::new(AccountId(7), Timestamp(Timestamp::now().seconds() + 3600), None, "fixture");
    sessions.put([1; 32], record.clone()).await.unwrap();
    let quiet = database.authority_domain.drain(Instant::now() + Duration::from_secs(5)).await.unwrap();
    let mut put = Box::pin(sessions.put([2; 32], record));
    pending(put.as_mut()).await;
    let accounts = database.accounts();
    let mut account = Box::pin(accounts.create_user(NewUser {
        name: "alice".parse().unwrap(),
        email: "alice@example.com".parse().unwrap(),
        display_name: None,
        password_hash: "unused".into(),
        is_admin: true,
    }));
    pending(account.as_mut()).await;
    assert_eq!(database.authority_domain.snapshot().queued, 2);
    assert!(kv.clone().sessions().get([1; 32]).await.unwrap().is_some());
    drop(put);
    assert_eq!(database.authority_domain.snapshot().queued, 1);
    assert!(sessions.get([2; 32]).await.unwrap().is_none());
    drop(clone.close());
    drop(quiet);
    assert!(matches!(account.await, Err(StorageError::Conflict("principal creation writer stopped"))));
    assert!(matches!(sessions.delete([1; 32]).await, Err(StorageError::Conflict("session writer stopped"))));
    assert!(matches!(kv.sessions().delete([1; 32]).await, Err(StorageError::Conflict("session writer stopped"))));
    drop(database.authority_domain.stop_writers().drain(Instant::now() + Duration::from_secs(5)).await.unwrap());
    assert!(sessions.get([1; 32]).await.unwrap().is_some());
    database.close().await;
}
