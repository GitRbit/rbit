//! Generic account rename SQL/redirect effects only, not full broker coverage.
use super::*;
use crate::{Database, sqlite::orgs::NewOrganization};
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
            password_hash: "old-hash".into(),
            is_admin: true,
        })
        .await
        .unwrap()
        .id()
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
        matches!(result, Err(StorageError::AdmissionUnavailable { scope: "account rename", reason: crate::AdmissionReason::Stopped })),
        "{result:?}"
    );
}
async fn mutation(store: &AccountStore, account: AccountId, conditional: bool) -> Result<(), StorageError> {
    let name: AccountName = "changed".parse().unwrap();
    if conditional { store.rename_if_password(account, &name, "old-hash").await.map(|_| ()) } else { store.rename(account, &name).await }
}

#[tokio::test]
async fn generic_user_and_org_renames_keep_ids_case_and_reclaimed_redirect_semantics() {
    let f = Fixture::new().await;
    let store = f.db.accounts();
    store.rename(f.alice, &"Alice".parse().unwrap()).await.unwrap();
    assert!(store.find_by_old_name("alice").await.unwrap().is_none());
    store.rename(f.alice, &"renamed".parse().unwrap()).await.unwrap();
    store.rename(f.alice, &"latest".parse().unwrap()).await.unwrap();
    for old in ["ALICE", "Renamed"] {
        let current = store.find_by_old_name(old).await.unwrap().unwrap();
        assert_eq!(current.id, f.alice);
        assert_eq!(current.name.as_str(), "latest");
        assert!(store.find_login(old).await.unwrap().is_none(), "redirects are not sign-in aliases");
    }
    store.rename(f.bob, &"alice".parse().unwrap()).await.unwrap();
    assert!(store.find_by_old_name("alice").await.unwrap().is_none());
    store.rename(f.bob, &"newcomer".parse().unwrap()).await.unwrap();
    assert_eq!(store.find_by_old_name("alice").await.unwrap().unwrap().id, f.bob);
    let org =
        f.db.orgs()
            .create(NewOrganization { name: "group".parse().unwrap(), display_name: None, description: None, creator: f.alice })
            .await
            .unwrap();
    store.rename(org.account.id, &"team".parse().unwrap()).await.unwrap();
    assert_eq!(store.find_by_old_name("group").await.unwrap().unwrap().id, org.account.id);
    assert_eq!(store.find_account_by_name("team").await.unwrap().unwrap().kind, AccountKind::Organization);
    assert!(!store.rename_if_password(org.account.id, &"unreachable".parse().unwrap(), "old-hash").await.unwrap());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn expected_password_suspension_and_missing_account_refusals_settle_cleanly() {
    let f = Fixture::new().await;
    let store = f.db.accounts();
    let name: AccountName = "renamed".parse().unwrap();
    assert!(!store.rename_if_password(f.alice, &name, "wrong").await.unwrap());
    f.db.admin().set_suspended(f.alice, true).await.unwrap();
    assert!(!store.rename_if_password(f.alice, &name, "old-hash").await.unwrap());
    f.db.admin().set_suspended(f.alice, false).await.unwrap();
    f.db.profiles().set_password_hash(f.alice, "new-hash").await.unwrap();
    assert!(!store.rename_if_password(f.alice, &name, "old-hash").await.unwrap());
    let missing = AccountId(i64::MAX);
    store.rename(missing, &name).await.unwrap();
    assert!(!store.rename_if_password(missing, &name, "new-hash").await.unwrap());
    assert!(store.find_account_by_name("alice").await.unwrap().is_some());
    assert!(store.find_by_old_name("alice").await.unwrap().is_none());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(store.rename_if_password(f.alice, &name, "new-hash").await.unwrap());
    assert!(store.rename_if_password(f.alice, &name, "new-hash").await.unwrap());
    assert_eq!(store.find_account_by_name("renamed").await.unwrap().unwrap().id, f.alice);
    assert_eq!(store.find_by_old_name("alice").await.unwrap().unwrap().id, f.alice);
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn duplicate_name_failure_preserves_trigger_history_and_allows_later_passive_rename() {
    let f = Fixture::new().await;
    let store = f.db.accounts();
    store.rename(f.alice, &"first".parse().unwrap()).await.unwrap();
    let taken: AccountName = "bob".parse().unwrap();
    assert!(matches!(store.rename(f.alice, &taken).await, Err(StorageError::Conflict("username"))));
    assert!(matches!(store.rename_if_password(f.alice, &taken, "old-hash").await, Err(StorageError::Conflict("username"))));
    assert_eq!(store.find_account_by_name("first").await.unwrap().unwrap().id, f.alice);
    assert_eq!(store.find_account_by_name("bob").await.unwrap().unwrap().id, f.bob);
    assert_eq!(store.find_by_old_name("alice").await.unwrap().unwrap().id, f.alice);
    assert!(store.find_by_old_name("first").await.unwrap().is_none());
    assert!(f.db.authority_domain.snapshot().uncertain);
    store.rename(f.alice, &"later".parse().unwrap()).await.unwrap();
    assert_eq!(store.find_by_old_name("first").await.unwrap().unwrap().name.as_str(), "later");
    assert!(matches!(f.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    f.db.close().await;
}

#[tokio::test]
async fn renames_share_policy_queue_and_terminal_old_fresh_handle_stop() {
    let f = Fixture::new().await;
    let clone = f.db.clone();
    let store = clone.accounts().clone();
    let policy = f.db.plugin_policy();
    let quiet = f.db.authority_domain.drain(deadline()).await.unwrap();
    let mut generic = Box::pin(mutation(&store, f.alice, false));
    let mut conditional = Box::pin(mutation(&store, f.alice, true));
    let mut review = Box::pin(policy.prepare(f.alice, Timestamp(1)));
    pending(generic.as_mut()).await;
    pending(conditional.as_mut()).await;
    pending(review.as_mut()).await;
    assert_eq!(clone.authority_domain.snapshot(), Snapshot { active: 0, queued: 3, uncertain: false, writers_stopped: false });
    assert!(store.find_account_by_name("alice").await.unwrap().is_some());
    drop(conditional);
    assert_eq!(f.db.authority_domain.snapshot().queued, 2);
    assert!(store.find_account_by_name("changed").await.unwrap().is_none());
    drop(clone.close());
    drop(f.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    stopped(&generic.await);
    assert!(matches!(
        review.await,
        Err(StorageError::AdmissionUnavailable { scope: "package policy", reason: crate::AdmissionReason::Stopped })
    ));
    for conditional in [false, true] {
        stopped(&mutation(&store, f.alice, conditional).await);
        stopped(&mutation(&f.db.accounts(), f.alice, conditional).await);
    }
    assert!(store.find_account_by_name("alice").await.unwrap().is_some());
    assert!(store.find_by_old_name("alice").await.unwrap().is_none());
    assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    f.db.close().await;
}

#[tokio::test]
async fn acquired_rename_cancellation_stays_uncertain_under_real_sql_writer_lock() {
    for conditional in [false, true] {
        let f = Fixture::new().await;
        let store = f.db.accounts();
        let mut connection = f.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation = Box::pin(mutation(&store, f.alice, conditional));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1);
        drop(operation);
        assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(f.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // Registration before first SQL await, not worker entry or known
        // settlement of a canceled driver's commit/rollback.
        f.db.close().await;
    }
}
