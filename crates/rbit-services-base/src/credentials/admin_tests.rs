//! Private real-storage owner tests; audit-tail gates are explicitly injected.
use super::*;
use rbit_core::{AccountId, Timestamp};
use rbit_storage::{Database, KvStore, NewUser, SessionRecord};
use std::{future::poll_fn, pin::Pin, sync::Arc, task::Poll, time::Duration};
use tokio::sync::Notify;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(5);
struct Fixture {
    _dir: tempfile::TempDir,
    db: Database,
    kv: KvStore,
    user: AccountId,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = db.open_kv(&dir.path().join("kv.redb")).unwrap();
        let user = db
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "old".into(),
                is_admin: true,
            })
            .await
            .unwrap()
            .id();
        kv.sessions().put([1; 32], SessionRecord::new(user, Timestamp(Timestamp::now().seconds() + 3600), None, "fixture")).await.unwrap();
        Self { _dir: dir, db, kv, user }
    }
    fn changes(&self) -> rbit_storage::sqlite::credential_changes::ForcedPasswordChanges {
        self.db.forced_password_changes(&self.kv).unwrap()
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn lost_waiter_keeps_two_real_storage_then_audit_tail_owners_bounded_and_drained() {
    let f = Fixture::new().await;
    let owner = AdminPasswordOwner::default();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut requests = Vec::new();
    for _ in 0..2 {
        let coordinator = owner.clone();
        let changes = f.changes();
        let owned_entered = entered.clone();
        let release = release.clone();
        let user = f.user;
        requests.push(tokio::spawn(async move {
            coordinator
                .run(&changes, move |reservation| async move {
                    reservation.start(user, "new".into())?.wait().await?;
                    let notified = release.notified();
                    tokio::pin!(notified);
                    notified.as_mut().enable();
                    owned_entered.notify_one(); // Real compound storage is acknowledged; injected audit tail follows.
                    timeout(WAIT, notified).await.map_err(ServiceError::internal)?;
                    Ok(())
                })
                .await
        }));
        timeout(WAIT, entered.notified()).await.unwrap();
    }
    assert_eq!(f.db.profiles().password_hash(f.user).await.unwrap().as_deref(), Some("new"));
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
    // Both storage slots are free, but the two audit tails must still bound admission.
    let third = owner.run(&f.changes(), |_| async { panic!("capacity refused before operation") }).await;
    assert!(matches!(third, Err(ServiceError::Conflict("administrator password command capacity"))));
    for request in requests {
        request.abort();
        assert!(timeout(WAIT, request).await.unwrap().unwrap_err().is_cancelled());
    }
    owner.prepare_shutdown();
    let mut drain = Box::pin(owner.shutdown());
    pending(drain.as_mut()).await;
    release.notify_waiters();
    timeout(WAIT, drain).await.unwrap().unwrap();
    assert_eq!(owner.inner.admission().active, 0);
    assert!(matches!(
        owner.run(&f.changes(), |_| async { Ok(()) }).await,
        Err(ServiceError::Conflict("administrator password commands stopped"))
    ));
}

#[tokio::test]
async fn private_task_panic_is_not_certified_by_an_empty_tracker() {
    let f = Fixture::new().await;
    let owner = AdminPasswordOwner::default();
    let result = owner
        .run(&f.changes(), |reservation| async move {
            drop(reservation); // Explicit known no storage was submitted in this injected failure.
            panic!("injected service owner panic");
        })
        .await;
    assert!(result.is_err());
    assert_eq!(owner.inner.admission().active, 0);
    assert!(owner.shutdown().await.is_err(), "empty tracker does not prove the service task acknowledged its result");
    assert_eq!(f.db.profiles().password_hash(f.user).await.unwrap().as_deref(), Some("old"));
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 1);
    f.db.prepare_forced_password_changes_shutdown();
    timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
}

#[tokio::test]
async fn close_and_registration_share_a_mutex_and_no_late_task_can_enter() {
    let f = Fixture::new().await;
    let owner = AdminPasswordOwner::default();
    owner.prepare_shutdown();
    for _ in 0..2 {
        assert!(matches!(
            owner.clone().run(&f.changes(), |_| async { panic!("closed before spawn") }).await,
            Err(ServiceError::Conflict("administrator password commands stopped"))
        ));
    }
    assert_eq!(owner.inner.admission().active, 0);
    timeout(WAIT, owner.shutdown()).await.unwrap().unwrap();
}

#[tokio::test]
async fn close_during_injected_hash_phase_returns_known_refusal_without_false_uncertainty() {
    let f = Fixture::new().await;
    let owner = AdminPasswordOwner::default();
    let (entered, entering) = tokio::sync::oneshot::channel();
    let (release, releasing) = tokio::sync::oneshot::channel();
    let own = owner.clone();
    let changes = f.changes();
    let user = f.user;
    let request = tokio::spawn(async move {
        own.run(&changes, move |reservation| async move {
            entered.send(()).unwrap();
            timeout(WAIT, releasing).await.map_err(ServiceError::internal)?.map_err(ServiceError::internal)?;
            reservation.start(user, "never".into())?.wait().await?;
            Ok(())
        })
        .await
    });
    timeout(WAIT, entering).await.unwrap().unwrap();
    owner.prepare_shutdown();
    f.db.prepare_forced_password_changes_shutdown();
    release.send(()).unwrap();
    assert!(matches!(timeout(WAIT, request).await.unwrap().unwrap(), Err(ServiceError::Conflict("forced password commands stopped"))));
    timeout(WAIT, owner.shutdown()).await.unwrap().unwrap();
    timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
    assert_eq!(f.db.profiles().password_hash(user).await.unwrap().as_deref(), Some("old"));
}

#[test]
fn entered_shutdown_runtime_releases_registered_task_without_locking_its_drop() {
    let (finished, receive) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        let live = tokio::runtime::Runtime::new().unwrap();
        let f = live.block_on(Fixture::new());
        let owner = AdminPasswordOwner::default();
        let changes = f.changes();
        let closed = tokio::runtime::Runtime::new().unwrap();
        let handle = closed.handle().clone();
        drop(closed);
        let mut request = Box::pin(owner.run(&changes, |_| async { panic!("closed runtime never polls task") }));
        {
            let _entered = handle.enter();
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(matches!(request.as_mut().poll(&mut context), Poll::Ready(Err(_))));
        }
        drop(request);
        assert_eq!(owner.inner.admission().active, 0);
        live.block_on(async {
            assert!(timeout(WAIT, owner.shutdown()).await.unwrap().is_err(), "lost task outcome stays uncertain");
            timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
            assert_eq!(f.db.profiles().password_hash(f.user).await.unwrap().as_deref(), Some("old"));
        });
        drop(owner);
        drop(changes);
        drop(f);
        drop(live);
        finished.send(()).unwrap();
    });
    receive.recv_timeout(WAIT).expect("task spawn deadlocked on a closed runtime");
    thread.join().unwrap();
}

#[tokio::test]
async fn waiter_loss_during_injected_hash_phase_keeps_owned_continuation_to_real_commits() {
    let f = Fixture::new().await;
    let owner = AdminPasswordOwner::default();
    let (entered, entering) = tokio::sync::oneshot::channel();
    let (release, releasing) = tokio::sync::oneshot::channel();
    let (completed, completing) = tokio::sync::oneshot::channel();
    let own = owner.clone();
    let changes = f.changes();
    let user = f.user;
    let request = tokio::spawn(async move {
        own.run(&changes, move |reservation| async move {
            entered.send(()).unwrap(); // Injected hash phase; no SQL submitted yet.
            timeout(WAIT, releasing).await.map_err(ServiceError::internal)?.map_err(ServiceError::internal)?;
            reservation.start(user, "new".into())?.wait().await?;
            completed.send(()).unwrap();
            Ok(())
        })
        .await
    });
    timeout(WAIT, entering).await.unwrap().unwrap();
    assert_eq!(f.db.profiles().password_hash(user).await.unwrap().as_deref(), Some("old"));
    request.abort();
    assert!(timeout(WAIT, request).await.unwrap().unwrap_err().is_cancelled());
    release.send(()).unwrap();
    timeout(WAIT, completing).await.unwrap().unwrap();
    timeout(WAIT, owner.shutdown()).await.unwrap().unwrap();
    timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
    assert_eq!(f.db.profiles().password_hash(user).await.unwrap().as_deref(), Some("new"));
    assert_eq!(f.kv.sessions().list_for_account(user).await.unwrap(), Vec::new());
}
