//! Real SQL/redb commits and retained workers. Gates/errors are injected, not crash proof.
use super::*;
use crate::sqlite::recovery::{NewEmailToken, TokenPurpose};
use crate::{Database, KvStore, NewUser, SessionRecord, kv::delete_account_sessions};
use rbit_core::Timestamp;
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::mpsc,
    task::Poll,
    time::Duration,
};
use tokio::time::{Instant, timeout};

const WAIT: Duration = Duration::from_secs(5);
struct Fixture {
    directory: tempfile::TempDir,
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
            .create_user_with_initial_password(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "old".into(),
                is_admin: true,
            })
            .await
            .unwrap()
            .id();
        for n in [1, 2] {
            kv.sessions()
                .put([n; 32], SessionRecord::new(user, Timestamp(Timestamp::now().seconds() + 3600), None, "fixture"))
                .await
                .unwrap();
        }
        Self { directory: dir, db, kv, user }
    }
    fn changes(&self) -> ForcedPasswordChanges {
        self.db.forced_password_changes(&self.kv).unwrap()
    }
    async fn hash(&self) -> String {
        sqlx::query_scalar("SELECT password_hash FROM user_logins WHERE account_id = ?")
            .bind(self.user.0)
            .fetch_one(&self.db.pool)
            .await
            .unwrap()
    }
    async fn links(&self) {
        for (n, purpose) in [(1, TokenPurpose::Reset), (2, TokenPurpose::Verify)] {
            self.db
                .recovery()
                .insert(&NewEmailToken {
                    digest: [n; 32],
                    account_id: self.user,
                    purpose,
                    email: "alice@example.com".parse().unwrap(),
                    expires_at: Timestamp(Timestamp::now().seconds() + 3600),
                })
                .await
                .unwrap();
        }
    }
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
        self.release.lock().unwrap().recv_timeout(WAIT).map_err(|_| StorageError::Conflict("injected worker gate closed"))
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
async fn gated_worker(
    sessions: SessionStore,
    account: AccountId,
    mutation: Mutation,
    command: Arc<Command>,
    gate: Arc<Gate>,
) -> Result<usize, StorageError> {
    sessions
        .finish_forced_password_with(account, mutation, command, move |db, account| {
            gate.wait()?;
            delete_account_sessions(db, account, None)
        })
        .await
}

#[tokio::test]
async fn acknowledged_replacement_clears_initial_flag_reset_links_and_session_index() {
    let f = Fixture::new().await;
    f.links().await;
    assert_eq!(f.changes().reserve().unwrap().start(f.user, "new".into()).unwrap().wait().await.unwrap(), 2);
    assert_eq!(f.hash().await, "new");
    assert!(!f.db.accounts().password_change_required(f.user).await.unwrap());
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_none());
    assert!(f.db.recovery().find_valid(&[2; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_some());
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
    assert_eq!(f.kv.sessions().delete_for_account(f.user, None).await.unwrap(), 0);
    assert_eq!(f.changes().reserve().unwrap().start(AccountId(i64::MAX), "missing".into()).unwrap().wait().await.unwrap(), 0);
    assert!(!f.db.authority_domain.snapshot().uncertain);
    f.db.prepare_forced_password_changes_shutdown();
    timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
}

#[tokio::test]
async fn reservation_bounds_and_terminal_start_are_clean_without_sql() {
    let f = Fixture::new().await;
    let changes = f.changes();
    let a = changes.reserve().unwrap();
    let b = changes.reserve().unwrap();
    assert!(matches!(changes.reserve(), Err(StorageError::Conflict("forced password command capacity"))));
    let close = f.db.close(); // synchronous terminal close, never polled here
    drop(close);
    assert!(matches!(a.start(f.user, "never".into()), Err(StorageError::Conflict("forced password commands stopped"))));
    let mut drain = Box::pin(f.db.drain_forced_password_changes());
    pending(drain.as_mut()).await; // forgotten reservations are still counted
    drop(b);
    timeout(WAIT, drain).await.unwrap().unwrap();
    assert_eq!(f.hash().await, "old");
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn receiver_loss_and_stop_between_commits_do_not_release_worker_or_reacquire() {
    let f = Fixture::new().await;
    let (gate, release) = Gate::new();
    let worker_gate = gate.clone();
    let result = f
        .changes()
        .reserve()
        .unwrap()
        .start_using(f.user, "new".into(), move |changes, command, account, hash| async move {
            run_with(changes, command, account, &hash, move |s, a, m, c| gated_worker(s, a, m, c, worker_gate)).await
        })
        .unwrap();
    timeout(WAIT, gate.entered.notified()).await.unwrap();
    assert_eq!(f.hash().await, "new", "actual SQL COMMIT precedes blocked redb worker");
    assert_eq!(f.db.authority_domain.snapshot().active, 1);
    let accounts = f.db.accounts();
    let name = "after".parse().unwrap();
    let mut unrelated = Box::pin(accounts.rename(f.user, &name));
    pending(unrelated.as_mut()).await;
    drop(unrelated);
    drop(result); // only receiver is lost; actual task is still alive
    let shutdown = f.db.authority_domain.stop_writers();
    f.db.prepare_forced_password_changes_shutdown();
    let mut drained = Box::pin(f.db.drain_forced_password_changes());
    pending(drained.as_mut()).await;
    release.send(()).unwrap();
    timeout(WAIT, drained).await.unwrap().unwrap();
    drop(timeout(WAIT, shutdown.drain(Instant::now() + WAIT)).await.unwrap().unwrap());
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn abort_async_owner_with_real_worker_retains_slot_and_sticky_uncertainty() {
    let f = Fixture::new().await;
    let changes = f.changes();
    let (gate, release) = Gate::new();
    let reservation = changes.reserve().unwrap();
    let ForcedPasswordReservation { changes: bound, command } = reservation;
    let worker_gate = gate.clone();
    let user = f.user;
    // Private test-only abort handle, never exposed by the public start API.
    let owner = tokio::spawn(run_with(bound, command, user, "new", move |s, a, m, c| gated_worker(s, a, m, c, worker_gate)));
    timeout(WAIT, gate.entered.notified()).await.unwrap();
    owner.abort();
    assert!(timeout(WAIT, owner).await.unwrap().unwrap_err().is_cancelled());
    assert_eq!(f.db.forced_passwords.inner.state().reserved, 1);
    assert!(f.db.forced_passwords.inner.state().uncertain);
    let other = changes.reserve().unwrap();
    assert!(matches!(changes.reserve(), Err(StorageError::Conflict("forced password command capacity"))));
    let mut drain = Box::pin(f.db.drain_forced_password_changes());
    pending(drain.as_mut()).await;
    release.send(()).unwrap();
    drop(other);
    assert!(timeout(WAIT, drain).await.unwrap().is_err());
    assert!(f.db.authority_domain.snapshot().uncertain, "late worker success cannot clear owner uncertainty");
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn actual_sql_commit_then_injected_worker_error_or_panic_is_not_rollback() {
    for panic_worker in [false, true] {
        let f = Fixture::new().await;
        let result = f
            .changes()
            .reserve()
            .unwrap()
            .start_using(f.user, "new".into(), move |changes, command, account, hash| async move {
                run_with(changes, command, account, &hash, move |sessions, account, mutation, command| async move {
                    sessions
                        .finish_forced_password_with(account, mutation, command, move |_, _| {
                            assert!(!panic_worker, "injected worker panic");
                            Err(StorageError::Conflict("injected worker failure"))
                        })
                        .await
                })
                .await
            })
            .unwrap()
            .wait()
            .await;
        assert!(result.is_err());
        assert_eq!(f.hash().await, "new");
        assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
        assert!(f.db.authority_domain.snapshot().uncertain);
        f.db.prepare_forced_password_changes_shutdown();
        assert!(timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().is_err());
    }
}

#[tokio::test]
async fn mismatch_and_queued_cancellation_never_submit_sql() {
    let f = Fixture::new().await;
    let other = Database::open(&f.directory.path().join("other.db")).await.unwrap();
    assert!(matches!(other.forced_password_changes(&f.kv), Err(StorageError::Conflict("session authority domain mismatch"))));
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let reservation = f.changes().reserve().unwrap();
    let ForcedPasswordReservation { changes, command } = reservation;
    let user = f.user;
    let mut owner = Box::pin(run_with(changes, command, user, "never", |sessions, account, mutation, command| async move {
        sessions.finish_forced_password(account, mutation, command).await
    }));
    pending(owner.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 1);
    drop(owner);
    drop(quiet);
    f.db.prepare_forced_password_changes_shutdown();
    timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
    assert!(!f.db.authority_domain.snapshot().uncertain);
    assert_eq!(f.hash().await, "old");
}

#[tokio::test]
async fn loss_after_sql_commit_before_kv_submission_remains_uncertain() {
    let f = Fixture::new().await;
    let (entered, entered_rx) = oneshot::channel();
    let reservation = f.changes().reserve().unwrap();
    let ForcedPasswordReservation { changes, command } = reservation;
    let user = f.user;
    let owner = tokio::spawn(run_with(changes, command, user, "new", move |sessions, account, mutation, command| async move {
        entered.send(()).unwrap();
        // Injected pre-submission suspension; the owning future still holds all
        // concrete inputs and Mutation. This is not a SQL worker-entry claim.
        std::future::pending::<()>().await;
        sessions.finish_forced_password(account, mutation, command).await
    }));
    timeout(WAIT, entered_rx).await.unwrap().unwrap();
    assert_eq!(f.hash().await, "new");
    owner.abort();
    assert!(timeout(WAIT, owner).await.unwrap().unwrap_err().is_cancelled());
    assert!(timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().is_err());
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
}

#[test]
fn start_without_current_runtime_is_known_prewrite_rejection() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let f = runtime.block_on(Fixture::new());
    let reservation = f.changes().reserve().unwrap();
    assert!(matches!(
        reservation.start(f.user, "never".into()),
        Err(StorageError::Conflict("forced password command runtime unavailable"))
    ));
    runtime.block_on(async {
        assert_eq!(f.hash().await, "old");
        f.db.prepare_forced_password_changes_shutdown();
        timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
        assert!(!f.db.authority_domain.snapshot().uncertain);
    });
}

#[test]
fn entered_shutdown_runtime_drops_admitted_task_without_admission_mutex_deadlock() {
    let (finished, receive) = mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        let live = tokio::runtime::Runtime::new().unwrap();
        let f = live.block_on(Fixture::new());
        let closed = tokio::runtime::Runtime::new().unwrap();
        let handle = closed.handle().clone();
        drop(closed);
        let reservation = f.changes().reserve().unwrap();
        let result = {
            let _entered = handle.enter();
            reservation.start(f.user, "never".into()).unwrap()
        };
        live.block_on(async {
            assert!(timeout(WAIT, result.wait()).await.unwrap().is_err());
            timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
            assert_eq!(f.hash().await, "old");
            assert!(!f.db.authority_domain.snapshot().uncertain);
        });
        drop(f);
        drop(live);
        finished.send(()).unwrap();
    });
    // A regression deadlock is a bounded test failure, never successful cleanup.
    receive.recv_timeout(WAIT).expect("admission spawn deadlocked on a closed runtime");
    thread.join().unwrap();
}
