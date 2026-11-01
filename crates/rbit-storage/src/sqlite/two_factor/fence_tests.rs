//! Factor SQL/replay state only; no KV or whole service settlement claim.
use super::*;
use crate::{Database, NewUser};
use rbit_authority_fence::{DrainError, Snapshot};
use sqlx::{
    Connection as _,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::Instant;

const SECRET: &[u8] = b"fixture-totp-secret";
const CODES: [[u8; 32]; 2] = [[1; 32], [2; 32]];
struct Fixture {
    dir: tempfile::TempDir,
    db: Database,
    alice: AccountId,
    bob: AccountId,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&db, "alice").await;
        let bob = user(&db, "bob").await;
        Self { dir, db, alice, bob }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db"))).await.unwrap()
    }
}
async fn user(db: &Database, name: &str) -> AccountId {
    db.accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: true,
        })
        .await
        .unwrap()
        .id()
}
fn passkey(id: &[u8]) -> NewPasskey<'_> {
    NewPasskey { credential_id: id, public_key: b"fixture-cose", algorithm: -7, sign_count: 5, transports: &[], name: "fixture" }
}
async fn seed(store: &TwoFactorStore, account: AccountId) -> i64 {
    store.begin_enrolment(account, SECRET).await.unwrap();
    store.confirm(account, SECRET, 10, &CODES).await.unwrap().unwrap();
    store.add_passkey(account, &passkey(b"seed"), &CODES).await.unwrap();
    store.find_passkey(account, b"seed").await.unwrap().unwrap().id
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
fn stopped(result: &Result<(), StorageError>) {
    assert!(
        matches!(result, Err(StorageError::AdmissionUnavailable { scope: "two-factor", reason: crate::AdmissionReason::Stopped })),
        "{result:?}"
    );
}
async fn mutation(store: &TwoFactorStore, account: AccountId, id: i64, kind: usize) -> Result<(), StorageError> {
    match kind {
        0 => store.begin_enrolment(account, SECRET).await.map(|_| ()),
        1 => store.confirm(account, SECRET, 11, &CODES).await.map(|_| ()),
        2 => store.claim_step(account, SECRET, 11).await.map(|_| ()),
        3 => store.spend_recovery_code(account, &CODES[0]).await.map(|_| ()),
        4 => store.replace_recovery_codes(account, &CODES).await.map(|_| ()),
        5 => store.cancel_enrolment(account).await,
        6 => store.disable(account).await.map(|_| ()),
        7 => store.add_passkey(account, &passkey(b"new"), &CODES).await.map(|_| ()),
        8 => store.use_passkey(account, id, 5, 6).await.map(|_| ()),
        _ => store.remove_passkey(account, id).await.map(|_| ()),
    }
}

#[tokio::test]
async fn totp_sealing_early_refusals_and_replay_state_settle_cleanly() {
    let f = Fixture::new().await;
    let store = f.db.two_factor();
    assert!(!store.replace_recovery_codes(f.alice, &CODES).await.unwrap());
    assert_eq!(store.confirm(f.alice, SECRET, 10, &CODES).await.unwrap(), None);
    assert!(!store.claim_step(f.alice, SECRET, 11).await.unwrap());
    store.cancel_enrolment(f.alice).await.unwrap();
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(store.begin_enrolment(f.alice, b"replaced").await.unwrap());
    assert!(store.begin_enrolment(f.alice, SECRET).await.unwrap());
    assert_eq!(store.find(f.alice).await.unwrap().unwrap().secret, SECRET);
    assert_eq!(store.confirm(f.alice, b"wrong", 10, &CODES).await.unwrap(), None);
    assert_eq!(store.confirm(f.alice, SECRET, 10, &CODES).await.unwrap(), Some(true));
    assert!(!store.begin_enrolment(f.alice, b"cannot-replace-enabled").await.unwrap());
    assert!(!store.claim_step(f.alice, SECRET, 10).await.unwrap());
    assert!(store.claim_step(f.alice, SECRET, 11).await.unwrap());
    assert!(!store.claim_step(f.alice, SECRET, 11).await.unwrap());
    assert!(store.spend_recovery_code(f.alice, &CODES[0]).await.unwrap());
    assert!(!store.spend_recovery_code(f.alice, &CODES[0]).await.unwrap());
    assert!(store.replace_recovery_codes(f.alice, &[[9; 32]]).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 1);
    store.cancel_enrolment(f.alice).await.unwrap();
    assert!(store.is_enabled(f.alice).await.unwrap());
    assert!(store.disable(f.alice).await.unwrap());
    assert!(!store.disable(f.alice).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 0);
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn passkey_counters_duplicates_and_first_last_factor_codes_remain_intact() {
    let f = Fixture::new().await;
    let store = f.db.two_factor();
    assert_eq!(store.add_passkey(f.alice, &passkey(b"first"), &CODES).await.unwrap(), PasskeyAdded::FirstFactor);
    assert_eq!(store.add_passkey(f.alice, &passkey(b"second"), &[[9; 32]]).await.unwrap(), PasskeyAdded::Added);
    assert_eq!(store.add_passkey(f.bob, &passkey(b"first"), &[[9; 32]]).await.unwrap(), PasskeyAdded::Duplicate);
    assert!(!store.is_enabled(f.bob).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.bob).await.unwrap(), 0);
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 2);
    let first = store.find_passkey(f.alice, b"first").await.unwrap().unwrap();
    let second = store.find_passkey(f.alice, b"second").await.unwrap().unwrap();
    assert!(store.use_passkey(f.alice, first.id, 5, 6).await.unwrap());
    assert!(!store.use_passkey(f.alice, first.id, 5, 7).await.unwrap());
    assert!(!store.use_passkey(f.bob, first.id, 6, 7).await.unwrap());
    assert_eq!(store.find_passkey(f.alice, b"first").await.unwrap().unwrap().sign_count, 6);
    assert!(!store.remove_passkey(f.bob, first.id).await.unwrap());
    store.begin_enrolment(f.alice, SECRET).await.unwrap();
    assert_eq!(store.confirm(f.alice, SECRET, 10, &[[9; 32]]).await.unwrap(), Some(false));
    assert!(store.remove_passkey(f.alice, first.id).await.unwrap());
    assert!(store.remove_passkey(f.alice, second.id).await.unwrap());
    assert!(!store.remove_passkey(f.alice, second.id).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 2);
    assert!(store.disable(f.alice).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 0);
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn ignored_confirmation_update_is_an_explicit_clean_rollback() {
    let f = Fixture::new().await;
    let store = f.db.two_factor();
    store.begin_enrolment(f.alice, SECRET).await.unwrap();
    let mut connection = f.connection().await;
    // Injected SQL no-op, not a claim of a real concurrent-writer/driver failure.
    sqlx::query("CREATE TRIGGER ignore_fixture_confirm BEFORE UPDATE OF confirmed_at ON two_factor BEGIN SELECT RAISE(IGNORE); END")
        .execute(&mut connection)
        .await
        .unwrap();
    assert_eq!(store.confirm(f.alice, SECRET, 10, &CODES).await.unwrap(), None);
    assert!(!store.is_enabled(f.alice).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 0);
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    sqlx::query("DROP TRIGGER ignore_fixture_confirm").execute(&mut connection).await.unwrap();
    assert_eq!(store.confirm(f.alice, SECRET, 10, &CODES).await.unwrap(), Some(true));
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn duplicate_recovery_code_errors_preserve_atomic_first_factor_changes() {
    let f = Fixture::new().await;
    let store = f.db.two_factor();
    let duplicates = [[7; 32]; 2];
    store.begin_enrolment(f.alice, SECRET).await.unwrap();
    assert!(store.confirm(f.alice, SECRET, 10, &duplicates).await.is_err());
    assert!(!store.is_enabled(f.alice).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 0);
    assert!(store.add_passkey(f.bob, &passkey(b"failed"), &duplicates).await.is_err());
    assert!(store.find_passkey(f.bob, b"failed").await.unwrap().is_none());
    assert_eq!(store.remaining_recovery_codes(f.bob).await.unwrap(), 0);
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert_eq!(store.confirm(f.alice, SECRET, 10, &CODES).await.unwrap(), Some(true));
    assert_eq!(store.add_passkey(f.bob, &passkey(b"later"), &CODES).await.unwrap(), PasskeyAdded::FirstFactor);
    assert!(matches!(f.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    f.db.close().await;
}

#[tokio::test]
async fn all_ten_factor_mutations_share_policy_queue_and_terminal_old_fresh_stop() {
    let f = Fixture::new().await;
    let clone = f.db.clone();
    let store = clone.two_factor().clone();
    let id = seed(&store, f.alice).await;
    let policy = f.db.plugin_policy();
    let quiet = f.db.authority_domain.drain(deadline()).await.unwrap();
    let mut operations: Vec<_> = (0..10).map(|kind| Box::pin(mutation(&store, f.alice, id, kind))).collect();
    for operation in &mut operations {
        pending(operation.as_mut()).await;
    }
    let mut review = Box::pin(policy.prepare(f.alice, Timestamp(1)));
    pending(review.as_mut()).await;
    assert_eq!(clone.authority_domain.snapshot(), Snapshot { active: 0, queued: 11, uncertain: false, writers_stopped: false });
    assert_eq!(store.find_passkey(f.alice, b"seed").await.unwrap().unwrap().id, id);
    drop(operations.pop().unwrap());
    assert_eq!(f.db.authority_domain.snapshot().queued, 10);
    assert!(store.find_passkey(f.alice, b"seed").await.unwrap().is_some());
    drop(clone.close());
    drop(f.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    for operation in operations {
        stopped(&operation.await);
    }
    assert!(matches!(
        review.await,
        Err(StorageError::AdmissionUnavailable { scope: "package policy", reason: crate::AdmissionReason::Stopped })
    ));
    for kind in 0..10 {
        stopped(&mutation(&store, f.alice, id, kind).await);
        stopped(&mutation(&f.db.two_factor(), f.alice, id, kind).await);
    }
    assert!(store.is_enabled(f.alice).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(f.alice).await.unwrap(), 2);
    assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    f.db.close().await;
}

#[tokio::test]
async fn all_ten_acquired_factor_cancellations_remain_uncertain_under_sql_writer_lock() {
    for kind in 0..10 {
        let f = Fixture::new().await;
        let store = f.db.two_factor();
        let id = seed(&store, f.alice).await;
        let mut connection = f.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation = Box::pin(mutation(&store, f.alice, id, kind));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1);
        drop(operation);
        assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(f.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // Enrollment before first SQL await, not actual worker entry or known
        // commit/rollback settlement of a canceled driver operation.
        f.db.close().await;
    }
}
