//! Real SQLite/redb acknowledgements. Private finite gates are injected, not crash proof.
use super::*;
use crate::{Database, KvStore, NewUser, SessionRecord, kv::delete_account_sessions};
use rbit_core::Timestamp;
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::{Mutex, mpsc},
    task::Poll,
    time::Duration,
};
use tokio::{
    sync::Notify,
    time::{Instant, timeout},
};

const WAIT: Duration = Duration::from_secs(5);
struct Fixture {
    directory: tempfile::TempDir,
    db: Database,
    kv: KvStore,
    user: AccountId,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("sql.db")).await.unwrap();
        let kv = db.open_kv(&directory.path().join("kv.redb")).unwrap();
        let user = new_user(&db, "alice").await;
        for n in [1, 2] {
            kv.sessions()
                .put([n; 32], SessionRecord::new(user, Timestamp(Timestamp::now().seconds() + 3600), None, "fixture"))
                .await
                .unwrap();
        }
        Self { directory, db, kv, user }
    }
    fn changes(&self) -> SelfPasswordChanges {
        self.db.self_password_changes(&self.kv).unwrap()
    }
    async fn pending(&self, new_hash: &str, expected: &str) -> PendingSelfPasswordChange {
        timeout(WAIT, self.changes().reserve().unwrap().start(self.user, new_hash.into(), expected.into()).unwrap().wait())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }
    async fn hash(&self) -> String {
        self.db.profiles().password_hash(self.user).await.unwrap().unwrap()
    }
}
async fn new_user(db: &Database, name: &str) -> AccountId {
    db.accounts()
        .create_user_with_initial_password(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "old".into(),
            is_admin: true,
        })
        .await
        .unwrap()
        .id()
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
struct Gate {
    entered: Notify,
    release: Mutex<mpsc::Receiver<()>>,
}
impl Gate {
    fn new() -> (Arc<Self>, mpsc::SyncSender<()>) {
        let (send, receive) = mpsc::sync_channel(1);
        (Arc::new(Self { entered: Notify::new(), release: Mutex::new(receive) }), send)
    }
    fn wait(&self) -> Result<(), StorageError> {
        self.entered.notify_one();
        self.release.lock().unwrap().recv_timeout(WAIT).map_err(|_| StorageError::Conflict("injected self worker gate closed"))
    }
}
async fn gated(sessions: SessionStore, worker: SelfPasswordWorker, gate: Arc<Gate>) -> Result<usize, StorageError> {
    sessions
        .finish_self_password_with(worker, move |db, account, keep| {
            gate.wait()?;
            delete_account_sessions(db, account, keep)
        })
        .await
}

#[tokio::test]
async fn sql_handoff_retains_active_and_allows_other_writers_before_end_others() {
    let f = Fixture::new().await;
    let change = f.pending("new", "old").await;
    assert_eq!(f.hash().await, "new");
    assert_eq!(f.db.authority_domain.snapshot().active, 1);
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
    f.db.profiles().set_email(f.user, &"changed@example.com".parse().unwrap()).await.unwrap();
    let mut quiet = Box::pin(f.db.authority_domain.drain(Instant::now() + WAIT));
    pending(quiet.as_mut()).await;
    drop(quiet);
    let result = f.changes().prepare_batch(vec![change], SelfPasswordAction::EndOthers { current: [1; 32] }).unwrap().start();
    assert_eq!(timeout(WAIT, result.wait()).await.unwrap().unwrap(), 1);
    assert!(f.kv.sessions().get([1; 32]).await.unwrap().is_some());
    assert!(f.kv.sessions().get([2; 32]).await.unwrap().is_none());
    assert_eq!(f.kv.sessions().delete_for_account(f.user, None).await.unwrap(), 1);
    assert!(!f.db.authority_domain.snapshot().uncertain);
    timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().unwrap();
}

#[tokio::test]
async fn two_commit_union_settles_one_real_batch_after_terminal_stop() {
    let f = Fixture::new().await;
    let a = f.pending("first", "old").await;
    let b = f.pending("second", "first").await;
    assert_eq!(f.db.authority_domain.snapshot().active, 2);
    let batch = f.changes().prepare_batch(vec![a, b], SelfPasswordAction::EndAll).unwrap();
    let (gate, release) = Gate::new();
    let worker_gate = gate.clone();
    let owner = tokio::spawn(run_batch(batch, move |sessions, worker| gated(sessions, worker, worker_gate)));
    timeout(WAIT, gate.entered.notified()).await.unwrap();
    let stopped = f.db.authority_domain.stop_writers();
    let mut drain = Box::pin(f.db.drain_self_password_changes());
    pending(drain.as_mut()).await;
    release.send(()).unwrap();
    assert_eq!(timeout(WAIT, owner).await.unwrap().unwrap().unwrap(), 2);
    timeout(WAIT, drain).await.unwrap().unwrap();
    drop(timeout(WAIT, stopped.drain(Instant::now() + WAIT)).await.unwrap().unwrap());
    assert_eq!(f.db.authority_domain.snapshot().active, 0);
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn paused_command_resumes_after_close_and_force_cannot_be_undone() {
    let f = Fixture::new().await;
    let change = f.pending("self", "old").await;
    assert_eq!(
        f.db.forced_password_changes(&f.kv).unwrap().reserve().unwrap().start(f.user, "forced".into()).unwrap().wait().await.unwrap(),
        2
    );
    let batch = f.changes().prepare_batch(vec![change], SelfPasswordAction::EndOthers { current: [1; 32] }).unwrap();
    drop(f.db.close()); // synchronous stop only; do not wait before admitted continuation
    assert_eq!(timeout(WAIT, batch.start().wait()).await.unwrap().unwrap(), 0);
    assert_eq!(f.hash().await, "forced");
    assert!(f.kv.sessions().get([1; 32]).await.unwrap().is_none());
    timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().unwrap();
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn conditional_false_is_acknowledged_without_kv_and_slots_are_independent() {
    let f = Fixture::new().await;
    let a = f.changes().reserve().unwrap();
    let b = f.changes().reserve().unwrap();
    assert!(matches!(
        f.changes().reserve(),
        Err(StorageError::AdmissionUnavailable { scope: "self password", reason: crate::AdmissionReason::Capacity })
    ));
    let forced = f.db.forced_password_changes(&f.kv).unwrap();
    let forced_a = forced.reserve().unwrap();
    let forced_b = forced.reserve().unwrap();
    assert!(matches!(
        forced.reserve(),
        Err(StorageError::AdmissionUnavailable { scope: "forced password", reason: crate::AdmissionReason::Capacity })
    ));
    assert!(timeout(WAIT, a.start(f.user, "never".into(), "stale".into()).unwrap().wait()).await.unwrap().unwrap().is_none());
    assert!(timeout(WAIT, b.start(AccountId(i64::MAX), "never".into(), "old".into()).unwrap().wait()).await.unwrap().unwrap().is_none());
    assert_eq!(f.hash().await, "old");
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
    drop((forced_a, forced_b));
    timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().unwrap();
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn failed_delivery_or_dropped_pending_after_real_sql_never_cleans_domain() {
    for lose_delivery in [true, false] {
        let f = Fixture::new().await;
        if lose_delivery {
            drop(f.changes().reserve().unwrap().start(f.user, "new".into(), "old".into()).unwrap());
        } else {
            drop(f.pending("new", "old").await);
        }
        assert!(timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().is_err());
        assert_eq!(f.hash().await, "new");
        assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
        assert!(f.db.authority_domain.snapshot().uncertain);
    }
}

#[tokio::test]
async fn immutable_receiver_factory_rejects_same_domain_different_kv_before_io() {
    let f = Fixture::new().await;
    let change = f.pending("new", "old").await;
    let other_kv = f.db.open_kv(&f.directory.path().join("other.redb")).unwrap();
    let other_factory = f.db.self_password_changes(&other_kv).unwrap();
    assert!(matches!(
        other_factory.prepare_batch(vec![change], SelfPasswordAction::EndAll),
        Err(StorageError::Invariant("self password batch binding mismatch"))
    ));
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
    assert_eq!(other_kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
    assert!(f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn mixed_accounts_and_domains_are_rejected_before_any_revocation() {
    for foreign_domain in [false, true] {
        let f = Fixture::new().await;
        let a = f.pending("new", "old").await;
        let other = Fixture::new().await;
        let b = if foreign_domain {
            other.pending("new", "old").await
        } else {
            let user = new_user(&f.db, "bob").await;
            f.changes().reserve().unwrap().start(user, "new".into(), "old".into()).unwrap().wait().await.unwrap().unwrap()
        };
        assert!(matches!(
            f.changes().prepare_batch(vec![a, b], SelfPasswordAction::EndAll),
            Err(StorageError::Invariant("self password batch binding mismatch"))
        ));
        assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
        assert_eq!(other.kv.sessions().list_for_account(other.user).await.unwrap().len(), 2);
        assert!(f.db.authority_domain.snapshot().uncertain);
    }
}

#[tokio::test]
async fn aborted_async_batch_poison_is_immediate_while_worker_keeps_slots() {
    let f = Fixture::new().await;
    let change = f.pending("new", "old").await;
    let batch = f.changes().prepare_batch(vec![change], SelfPasswordAction::EndAll).unwrap();
    let (gate, release) = Gate::new();
    let worker_gate = gate.clone();
    let owner = tokio::spawn(run_batch(batch, move |sessions, worker| gated(sessions, worker, worker_gate)));
    timeout(WAIT, gate.entered.notified()).await.unwrap();
    owner.abort();
    assert!(timeout(WAIT, owner).await.unwrap().unwrap_err().is_cancelled());
    assert!(f.db.self_passwords.inner.state().uncertain);
    assert_eq!(f.db.self_passwords.inner.state().reserved, 1);
    assert_eq!(f.db.authority_domain.snapshot().active, 1);
    let spare = f.changes().reserve().unwrap();
    assert!(f.changes().reserve().is_err());
    let mut drain = Box::pin(f.db.drain_self_password_changes());
    pending(drain.as_mut()).await;
    release.send(()).unwrap();
    drop(spare);
    assert!(timeout(WAIT, drain).await.unwrap().is_err());
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn worker_error_and_panic_after_sql_are_uncertain_not_rollback() {
    for panic_worker in [false, true] {
        let f = Fixture::new().await;
        let change = f.pending("new", "old").await;
        let batch = f.changes().prepare_batch(vec![change], SelfPasswordAction::EndAll).unwrap();
        let result = run_batch(batch, |sessions, worker| async move {
            sessions
                .finish_self_password_with(worker, move |_, _, _| {
                    assert!(!panic_worker, "injected self worker panic");
                    Err(StorageError::Conflict("injected self worker failure"))
                })
                .await
        })
        .await;
        assert!(result.is_err());
        assert_eq!(f.hash().await, "new");
        assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
        assert!(timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().is_err());
        assert!(f.db.authority_domain.snapshot().uncertain);
    }
}

#[tokio::test]
async fn cancellation_before_acquisition_is_clean_but_held_sql_lock_loss_is_unknown() {
    let f = Fixture::new().await;
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let SelfPasswordReservation { changes, command } = f.changes().reserve().unwrap();
    let mut queued = Box::pin(run_sql(changes, command, f.user, "never", "old", tokio::runtime::Handle::current()));
    pending(queued.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 1);
    drop(queued);
    drop(quiet);
    assert!(!f.db.authority_domain.snapshot().uncertain);
    let mut lock = f.db.pool.acquire().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
    let SelfPasswordReservation { changes, command } = f.changes().reserve().unwrap();
    let mut acquired = Box::pin(run_sql(changes, command, f.user, "unknown", "old", tokio::runtime::Handle::current()));
    pending(acquired.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().active, 1, "registered while SQL writer lock is held, not worker-entry proof");
    drop(acquired);
    sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
    assert!(timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().is_err());
    assert!(f.db.authority_domain.snapshot().uncertain);
}

#[test]
fn closed_runtime_dispatch_is_clean_before_sql_and_unknown_after_pending_handoff() {
    let (finished, receive) = mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        let live = tokio::runtime::Runtime::new().unwrap();
        let f = live.block_on(Fixture::new());
        let closed = tokio::runtime::Runtime::new().unwrap();
        let handle = closed.handle().clone();
        drop(closed);
        let result = {
            let _entered = handle.enter();
            f.changes().reserve().unwrap().start(f.user, "never".into(), "old".into()).unwrap()
        };
        live.block_on(async {
            assert!(timeout(WAIT, result.wait()).await.unwrap().is_err());
            assert_eq!(f.hash().await, "old");
            assert!(!f.db.authority_domain.snapshot().uncertain);
            assert_eq!(f.db.self_passwords.inner.state().reserved, 0);
            let mut change = f.pending("new", "old").await;
            // Inject exactly a captured runtime that has already shut down, AFTER
            // real SQL handoff. No retry onto live is permitted for finalization.
            change.runtime = handle.clone();
            let batch = f.changes().prepare_batch(vec![change], SelfPasswordAction::EndAll).unwrap();
            let waiter = batch.start();
            assert!(timeout(WAIT, waiter.wait()).await.unwrap().is_err());
            assert_eq!(f.hash().await, "new");
            assert!(timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().is_err());
            assert!(f.db.authority_domain.snapshot().uncertain);
            assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
        });
        drop(f);
        drop(live);
        finished.send(()).unwrap();
    });
    receive.recv_timeout(WAIT).expect("closed-runtime dispatch deadlocked");
    thread.join().unwrap();
}

#[tokio::test]
async fn dropped_final_receiver_still_settles_real_worker_and_terminal_refuses_stale_start() {
    let f = Fixture::new().await;
    let pending = f.pending("new", "old").await;
    let reservation = f.changes().reserve().unwrap();
    let batch = f.changes().prepare_batch(vec![pending], SelfPasswordAction::EndAll).unwrap();
    f.db.prepare_self_password_changes_shutdown();
    assert!(matches!(
        reservation.start(f.user, "never".into(), "new".into()),
        Err(StorageError::AdmissionUnavailable { scope: "self password", reason: crate::AdmissionReason::Stopped })
    ));
    assert!(f.db.clone().self_password_changes(&f.kv).unwrap().reserve().is_err());
    drop(batch.start());
    timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().unwrap();
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn canceling_already_acquired_resume_is_unknown_without_submitting_kv() {
    let f = Fixture::new().await;
    let change = f.pending("new", "old").await;
    let serial = f.db.authority_domain.writer().begin().await.unwrap();
    let batch = f.changes().prepare_batch(vec![change], SelfPasswordAction::EndAll).unwrap();
    let mut owner = Box::pin(run_batch(batch, |sessions, worker| async move { sessions.finish_self_password(worker).await }));
    pending(owner.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().active, 2);
    drop(owner);
    serial.complete();
    assert!(timeout(WAIT, f.db.drain_self_password_changes()).await.unwrap().is_err());
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
}
