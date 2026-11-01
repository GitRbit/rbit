//! Actual reset SQL and retained redb workers. Faults/gates are explicitly injected.
use super::*;
use crate::{
    Database, KvStore, NewUser, SessionRecord,
    kv::delete_account_sessions,
    sqlite::recovery::{NewEmailToken, TokenPurpose},
};
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
    other: AccountId,
}
impl Fixture {
    async fn new() -> Self {
        let (dir, db) = super::super::plans::database().await;
        let kv = db.open_kv(&dir.path().join("kv.redb")).unwrap();
        let user = create_user(&db, "alice").await;
        let other = create_user(&db, "bob").await;
        for (n, account) in [(1, user), (2, user), (3, other)] {
            kv.sessions()
                .put([n; 32], SessionRecord::new(account, Timestamp(Timestamp::now().seconds() + 3600), None, "fixture"))
                .await
                .unwrap();
        }
        Self { directory: dir, db, kv, user, other }
    }
    fn changes(&self) -> ForcedPasswordChanges {
        self.db.forced_password_changes(&self.kv).unwrap()
    }
    async fn token(&self, n: u8, purpose: TokenPurpose) -> NewEmailToken {
        let token = NewEmailToken {
            digest: [n; 32],
            account_id: self.user,
            purpose,
            email: "alice@example.com".parse().unwrap(),
            expires_at: Timestamp(Timestamp::now().seconds() + 3600),
        };
        self.db.recovery().insert(&token).await.unwrap();
        token
    }
    async fn hash(&self) -> String {
        self.db.profiles().password_hash(self.user).await.unwrap().unwrap()
    }
}
async fn create_user(db: &Database, name: &str) -> AccountId {
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
async fn refused(f: &Fixture, digest: [u8; 32], now: Timestamp) {
    let result = f
        .changes()
        .reserve()
        .unwrap()
        .start_operation(move |changes, command| async move {
            run_reset_with(changes, command, &digest, "never", now, |_, _, _, _| async {
                panic!("known rolled-back None must not submit a KV continuation")
            })
            .await
        })
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert_eq!(result, None);
    assert_eq!(f.hash().await, "old");
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
    drop(f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap());
}
struct Gate {
    entered: Notify,
    release: Mutex<mpsc::Receiver<()>>,
}
impl Gate {
    fn new() -> (Arc<Self>, mpsc::SyncSender<()>) {
        let (send, recv) = mpsc::sync_channel(1);
        (Arc::new(Self { entered: Notify::new(), release: Mutex::new(recv) }), send)
    }
    fn wait(&self) -> Result<(), StorageError> {
        self.entered.notify_one();
        self.release.lock().unwrap().recv_timeout(WAIT).map_err(|_| StorageError::Conflict("injected reset worker gate closed"))
    }
}

#[tokio::test]
async fn concurrent_same_or_competing_reset_links_have_one_committed_winner() {
    for same_digest in [false, true] {
        let f = Fixture::new().await;
        f.token(1, TokenPurpose::Reset).await;
        f.token(2, TokenPurpose::Reset).await;
        f.token(3, TokenPurpose::Verify).await;
        let now = Timestamp::now();
        let changes = f.changes();
        let a = changes.reserve().unwrap().start_reset([1; 32], "a".into(), now).unwrap();
        let b = changes.reserve().unwrap().start_reset([if same_digest { 1 } else { 2 }; 32], "b".into(), now).unwrap();
        let (a, b) = timeout(WAIT, async { tokio::join!(a.wait(), b.wait()) }).await.unwrap();
        let a = a.unwrap();
        let b = b.unwrap();
        assert_ne!(a.is_some(), b.is_some());
        let winning_hash = if a.is_some() { "a" } else { "b" };
        let winner = a.or(b).unwrap();
        assert_eq!(winner.token.account_id, f.user);
        assert_eq!(winner.removed_sessions, 2);
        assert_eq!(f.hash().await, winning_hash);
        assert!(!f.db.accounts().password_change_required(f.user).await.unwrap());
        assert_eq!(f.db.recovery().find_account(f.user).await.unwrap().unwrap().email_verified_at, Some(now));
        assert!(f.db.recovery().find_valid(&[2; 32], TokenPurpose::Reset, now).await.unwrap().is_none());
        assert!(f.db.recovery().find_valid(&[3; 32], TokenPurpose::Verify, now).await.unwrap().is_some());
        assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
        assert_eq!(f.kv.sessions().list_for_account(f.other).await.unwrap().len(), 1);
        timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn purpose_expiry_used_email_and_suspension_refusals_rollback_without_kv() {
    for case in 0..7 {
        let f = Fixture::new().await;
        let token = f.token(1, if case == 0 { TokenPurpose::Verify } else { TokenPurpose::Reset }).await;
        let now = if case == 1 { token.expires_at } else { Timestamp::now() };
        match case {
            2 => {
                f.db.recovery().consume(&token.digest, TokenPurpose::Reset, now).await.unwrap().unwrap();
            }
            3 => {
                // Deliberately raw fixture update: isolate honoured!'s current-email predicate, not token invalidation helper.
                sqlx::query("UPDATE user_logins SET email = 'changed@example.com' WHERE account_id = ?")
                    .bind(f.user.0)
                    .execute(&f.db.pool)
                    .await
                    .unwrap();
            }
            4 => {
                f.db.admin().set_suspended(f.user, true).await.unwrap();
            }
            5 => {
                // Deliberately raw fixture removes login while retaining the account/token.
                sqlx::query("DELETE FROM user_logins WHERE account_id = ?").bind(f.user.0).execute(&f.db.pool).await.unwrap();
            }
            _ => {}
        }
        if case == 5 {
            let result = f.changes().reserve().unwrap().start_reset(token.digest, "never".into(), now).unwrap().wait().await.unwrap();
            assert_eq!(result, None);
            assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
            assert!(!f.db.authority_domain.snapshot().uncertain);
        } else {
            refused(&f, if case == 6 { [99; 32] } else { token.digest }, now).await;
        }
        timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn reset_uses_exact_captured_timestamp_and_refusal_does_not_consume_link() {
    let f = Fixture::new().await;
    let token = f.token(1, TokenPurpose::Reset).await;
    refused(&f, token.digest, token.expires_at).await;
    let captured = Timestamp(token.expires_at.seconds() - 1);
    let result = f.changes().reserve().unwrap().start_reset(token.digest, "new".into(), captured).unwrap().wait().await.unwrap().unwrap();
    assert_eq!(result.token.expires_at, token.expires_at);
    assert_eq!(f.db.recovery().find_account(f.user).await.unwrap().unwrap().email_verified_at, Some(captured));
    // Supplied capture is used exactly, not replaced by worker wall clock; no new freshness guarantee.
    timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
}

#[tokio::test]
async fn lost_reset_receiver_shares_admin_quota_and_finishes_after_terminal_close() {
    let f = Fixture::new().await;
    f.token(1, TokenPurpose::Reset).await;
    let (gate, release) = Gate::new();
    let worker_gate = gate.clone();
    let now = Timestamp::now();
    let result = f
        .changes()
        .reserve()
        .unwrap()
        .start_operation(move |changes, command| async move {
            run_reset_with(changes, command, &[1; 32], "new", now, move |sessions, account, mutation, command| async move {
                sessions
                    .finish_forced_password_with(account, mutation, command, move |db, account| {
                        worker_gate.wait()?;
                        delete_account_sessions(db, account, None)
                    })
                    .await
            })
            .await
        })
        .unwrap();
    timeout(WAIT, gate.entered.notified()).await.unwrap();
    assert_eq!(f.hash().await, "new");
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, now).await.unwrap().is_none());
    let other = f.changes().reserve().unwrap();
    assert!(matches!(
        f.changes().reserve(),
        Err(StorageError::AdmissionUnavailable { scope: "forced password", reason: crate::AdmissionReason::Capacity })
    ));
    let accounts = f.db.accounts();
    let name = "after".parse().unwrap();
    let mut unrelated = Box::pin(accounts.rename(f.user, &name));
    pending(unrelated.as_mut()).await;
    drop(unrelated);
    drop(result);
    let stop = f.db.authority_domain.stop_writers();
    let mut drained = Box::pin(f.db.drain_forced_password_changes());
    pending(drained.as_mut()).await;
    drop(other);
    release.send(()).unwrap();
    timeout(WAIT, drained).await.unwrap().unwrap();
    drop(timeout(WAIT, stop.drain(Instant::now() + WAIT)).await.unwrap().unwrap());
    assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap(), Vec::new());
    assert_eq!(f.kv.sessions().list_for_account(f.other).await.unwrap().len(), 1);
}

#[tokio::test]
async fn committed_reset_then_injected_kv_failure_consumes_link_without_false_success() {
    for panic_worker in [false, true] {
        let f = Fixture::new().await;
        f.token(1, TokenPurpose::Reset).await;
        let now = Timestamp::now();
        let result = f
            .changes()
            .reserve()
            .unwrap()
            .start_operation(move |changes, command| async move {
                run_reset_with(changes, command, &[1; 32], "new", now, move |sessions, account, mutation, command| async move {
                    sessions
                        .finish_forced_password_with(account, mutation, command, move |_, _| {
                            assert!(!panic_worker, "injected reset KV panic");
                            Err(StorageError::Conflict("injected reset KV failure"))
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
        assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, now).await.unwrap().is_none());
        assert!(f.db.authority_domain.snapshot().uncertain);
        assert!(timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().is_err());
    }
}

#[tokio::test]
async fn email_and_suspension_changes_before_sql_write_defeat_preflight_without_kv() {
    use sqlx::Connection as _;
    for suspend in [false, true] {
        let f = Fixture::new().await;
        let token = f.token(1, TokenPurpose::Reset).await;
        let now = Timestamp::now();
        assert!(f.db.recovery().find_valid(&token.digest, TokenPurpose::Reset, now).await.unwrap().is_some());
        let mut connection = sqlx::sqlite::SqliteConnection::connect_with(
            &sqlx::sqlite::SqliteConnectOptions::new().filename(f.directory.path().join("rbit.db")),
        )
        .await
        .unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let ForcedPasswordReservation { changes, command } = f.changes().reserve().unwrap();
        let digest = token.digest;
        let mut reset = Box::pin(run_reset_with(changes, command, &digest, "never", now, |_, _, _, _| async {
            panic!("write-time refused reset cannot enter KV")
        }));
        pending(reset.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1);
        // Raw held-writer fixture commits after preflight and before the reset
        // can write. Registration does not prove SQL worker entry.
        let sql = if suspend {
            "UPDATE user_logins SET suspended_at = 1 WHERE account_id = ?"
        } else {
            "UPDATE user_logins SET email = 'changed@example.com' WHERE account_id = ?"
        };
        sqlx::query(sql).bind(f.user.0).execute(&mut connection).await.unwrap();
        sqlx::query("COMMIT").execute(&mut connection).await.unwrap();
        assert_eq!(timeout(WAIT, reset).await.unwrap().unwrap(), None);
        assert_eq!(f.hash().await, "old");
        assert_eq!(f.kv.sessions().list_for_account(f.user).await.unwrap().len(), 2);
        timeout(WAIT, f.db.drain_forced_password_changes()).await.unwrap().unwrap();
    }
}
