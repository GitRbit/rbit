//! PAT issuance/revocation only; last-used bookkeeping remains unenrolled.
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
        let alice = user(&db, "alice", true).await;
        let bob = user(&db, "bob", false).await;
        Self { dir, db, alice, bob }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db"))).await.unwrap()
    }
}
async fn user(db: &Database, name: &str, admin: bool) -> AccountId {
    db.accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: admin,
        })
        .await
        .unwrap()
        .id()
}
fn token(owner: AccountId, digest: u8) -> NewAccessToken {
    NewAccessToken {
        account_id: owner,
        name: format!("fixture-{digest}"),
        digest: [digest; 32],
        hint: "fixture".into(),
        scope: TokenScope::Admin,
        expires_at: Some(Timestamp(4_000_000_000)),
    }
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
fn stopped<T: std::fmt::Debug>(result: &Result<T, StorageError>) {
    assert!(
        matches!(result, Err(StorageError::AdmissionUnavailable { scope: "access token", reason: crate::AdmissionReason::Stopped })),
        "{result:?}"
    );
}
async fn mutation(store: &AccessTokenStore, owner: AccountId, id: AccessTokenId, kind: usize) -> Result<(), StorageError> {
    match kind {
        0 => store.create(token(owner, 2)).await.map(|_| ()),
        1 => store.create_with_limit(token(owner, 3), 10).await.map(|_| ()),
        _ => store.delete(owner, id).await.map(|_| ()),
    }
}

#[tokio::test]
async fn issuance_cap_refusal_and_owner_checked_deletion_all_settle_cleanly() {
    let fixture = Fixture::new().await;
    let store = fixture.db.access_tokens();
    let first = store.create(token(fixture.alice, 1)).await.unwrap();
    assert_eq!(first.account_id, fixture.alice);
    assert_eq!(first.scope, TokenScope::Admin);
    assert_eq!(first.expires_at, Some(Timestamp(4_000_000_000)));
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(store.create_with_limit(token(fixture.alice, 2), 1).await.unwrap().is_none());
    assert!(store.find_by_digest(&[2; 32]).await.unwrap().is_none());
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    let second = store.create_with_limit(token(fixture.alice, 3), 2).await.unwrap().unwrap();
    assert_eq!(second.account_id, fixture.alice);
    assert!(!store.delete(fixture.bob, first.id).await.unwrap());
    assert!(store.find_by_digest(&[1; 32]).await.unwrap().is_some());
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(store.delete(fixture.alice, first.id).await.unwrap());
    assert!(!store.delete(fixture.alice, first.id).await.unwrap());
    assert_eq!(store.count_for_account(fixture.alice).await.unwrap(), 1);
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    fixture.db.close().await;
}

#[tokio::test]
async fn token_paths_share_policy_serial_and_terminal_stop_without_blocking_reads() {
    let fixture = Fixture::new().await;
    let clone = fixture.db.clone();
    let store = clone.access_tokens().clone();
    let first = store.create(token(fixture.alice, 1)).await.unwrap();
    let policy = fixture.db.plugin_policy();
    let quiet = fixture.db.authority_domain.drain(deadline()).await.unwrap();
    let mut create = Box::pin(store.create(token(fixture.alice, 2)));
    let mut limited = Box::pin(store.create_with_limit(token(fixture.alice, 3), 10));
    let mut delete = Box::pin(store.delete(fixture.alice, first.id));
    let mut review = Box::pin(policy.prepare(fixture.alice, Timestamp(1)));
    pending(create.as_mut()).await;
    pending(limited.as_mut()).await;
    pending(delete.as_mut()).await;
    pending(review.as_mut()).await;
    assert_eq!(clone.authority_domain.snapshot(), Snapshot { active: 0, queued: 4, uncertain: false, writers_stopped: false });
    assert_eq!(store.count_for_account(fixture.alice).await.unwrap(), 1);
    drop(create);
    assert_eq!(fixture.db.authority_domain.snapshot().queued, 3);
    assert!(store.find_by_digest(&[2; 32]).await.unwrap().is_none());
    drop(clone.close());
    drop(fixture.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    stopped(&limited.await);
    stopped(&delete.await);
    assert!(matches!(
        review.await,
        Err(StorageError::AdmissionUnavailable { scope: "package policy", reason: crate::AdmissionReason::Stopped })
    ));
    for kind in 0..3 {
        stopped(&mutation(&store, fixture.alice, first.id, kind).await);
        stopped(&mutation(&fixture.db.access_tokens(), fixture.alice, first.id, kind).await);
    }
    assert_eq!(store.count_for_account(fixture.alice).await.unwrap(), 1);
    assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    fixture.db.close().await;
}

#[tokio::test]
async fn each_acquired_pat_mutation_cancel_is_uncertain_with_real_sql_writer_held() {
    for kind in 0..3 {
        let fixture = Fixture::new().await;
        let store = fixture.db.access_tokens();
        let first = store.create(token(fixture.alice, 1)).await.unwrap();
        let mut connection = fixture.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation = Box::pin(mutation(&store, fixture.alice, first.id, kind));
        pending(operation.as_mut()).await;
        assert_eq!(fixture.db.authority_domain.snapshot().active, 1);
        drop(operation);
        assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(fixture.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // Enrollment before the first SQL await, not SQLite worker-entry or a
        // known settlement outcome for the canceled driver command.
        fixture.db.close().await;
    }
}

#[tokio::test]
async fn duplicate_error_stays_uncertain_but_later_passive_issuance_remains_available() {
    let fixture = Fixture::new().await;
    let store = fixture.db.access_tokens();
    store.create(token(fixture.alice, 1)).await.unwrap();
    assert!(matches!(store.create(token(fixture.alice, 1)).await, Err(StorageError::Conflict("token"))));
    assert!(fixture.db.authority_domain.snapshot().uncertain);
    store.create_with_limit(token(fixture.alice, 2), 10).await.unwrap().unwrap();
    assert_eq!(store.count_for_account(fixture.alice).await.unwrap(), 2);
    assert!(matches!(fixture.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    fixture.db.close().await;
}

#[tokio::test]
async fn create_final_row_not_found_remains_inside_its_single_guard() {
    let fixture = Fixture::new().await;
    let store = fixture.db.access_tokens();
    let mut connection = fixture.connection().await;
    // Injected SQL fixture: force a no-row INSERT result even at i64::MAX.
    // This is not an actual quota-exhaustion or driver-failure simulation.
    sqlx::query("CREATE TRIGGER refuse_fixture_token BEFORE INSERT ON access_tokens BEGIN SELECT RAISE(IGNORE); END")
        .execute(&mut connection)
        .await
        .unwrap();
    assert!(store.create_with_limit(token(fixture.alice, 1), 10).await.unwrap().is_none());
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(matches!(store.create(token(fixture.alice, 2)).await, Err(StorageError::Sqlite(sqlx::Error::RowNotFound))));
    assert!(fixture.db.authority_domain.snapshot().uncertain);
    assert_eq!(store.count_for_account(fixture.alice).await.unwrap(), 0);
    sqlx::query("DROP TRIGGER refuse_fixture_token").execute(&mut connection).await.unwrap();
    store.create(token(fixture.alice, 3)).await.unwrap();
    assert!(matches!(fixture.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    fixture.db.close().await;
}
