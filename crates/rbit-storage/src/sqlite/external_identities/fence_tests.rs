//! Link/unlink enrollment; reported sign-in metadata remains outside this category.
use super::*;
use crate::Database;
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
        let alice = user(&db, "alice", "hash", true).await;
        let bob = user(&db, "bob", "", false).await;
        Self { dir, db, alice, bob }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db"))).await.unwrap()
    }
}
async fn user(db: &Database, name: &str, hash: &str, admin: bool) -> AccountId {
    db.accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: hash.into(),
            is_admin: admin,
        })
        .await
        .unwrap()
        .id()
}
fn identity<'a>(provider: &'a str, subject: &'a str) -> NewExternalIdentity<'a> {
    NewExternalIdentity { provider_id: provider, subject, email: None, username: None }
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
    assert!(matches!(result, Err(StorageError::Conflict("external identity linking writer stopped"))), "{result:?}");
}

#[tokio::test]
async fn linked_success_and_both_business_conflicts_settle_cleanly_without_changed_binding() {
    let fixture = Fixture::new().await;
    let store = fixture.db.external_identities();
    let original = store.link(fixture.alice, identity("corp", "alice"), Timestamp(1)).await.unwrap().unwrap();
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    assert_eq!(store.link(fixture.bob, identity("corp", "alice"), Timestamp(2)).await.unwrap(), Err(LinkConflict::IdentityTaken));
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    assert_eq!(store.link(fixture.alice, identity("corp", "other"), Timestamp(3)).await.unwrap(), Err(LinkConflict::ProviderTaken));
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    assert_eq!(store.find("corp", "alice").await.unwrap(), Some(original.clone()));
    assert_eq!(store.list_for_account(fixture.alice).await.unwrap(), [original]);
    assert_eq!(store.list_for_account(fixture.bob).await.unwrap(), Vec::<ExternalIdentity>::new());
    assert!(store.find("corp", "other").await.unwrap().is_none());
    fixture.db.close().await;
}

#[tokio::test]
async fn unlink_commit_preserves_removed_last_method_and_missing_outcomes() {
    let fixture = Fixture::new().await;
    let store = fixture.db.external_identities();
    let usable = ["corp", "github"];
    store.link(fixture.alice, identity("corp", "alice"), Timestamp(1)).await.unwrap().unwrap();
    assert_eq!(store.unlink(fixture.alice, "corp", &usable).await.unwrap(), Unlinked::Removed);
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    assert_eq!(store.unlink(fixture.alice, "corp", &usable).await.unwrap(), Unlinked::NotFound);
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    store.link(fixture.bob, identity("corp", "bob"), Timestamp(1)).await.unwrap().unwrap();
    store.link(fixture.bob, identity("retired", "bob"), Timestamp(1)).await.unwrap().unwrap();
    assert_eq!(store.unlink(fixture.bob, "corp", &usable).await.unwrap(), Unlinked::LastMethod);
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    store.link(fixture.bob, identity("github", "bob"), Timestamp(1)).await.unwrap().unwrap();
    assert_eq!(store.unlink(fixture.bob, "corp", &usable).await.unwrap(), Unlinked::Removed);
    assert_eq!(store.unlink(fixture.bob, "github", &usable).await.unwrap(), Unlinked::LastMethod);
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    fixture.db.close().await;
}

#[tokio::test]
async fn shared_queue_cancel_and_terminal_stop_cover_old_and_fresh_linking_handles() {
    let fixture = Fixture::new().await;
    let clone = fixture.db.clone();
    let store = clone.external_identities().clone();
    let original = store.link(fixture.alice, identity("corp", "alice"), Timestamp(1)).await.unwrap().unwrap();
    let policy = fixture.db.plugin_policy();
    let usable = ["corp"];
    let quiet = fixture.db.authority_domain.drain(deadline()).await.unwrap();
    let mut link = Box::pin(store.link(fixture.bob, identity("github", "bob"), Timestamp(2)));
    let mut unlink = Box::pin(store.unlink(fixture.alice, "corp", &usable));
    let mut review = Box::pin(policy.prepare(fixture.alice, Timestamp(1)));
    pending(link.as_mut()).await;
    pending(unlink.as_mut()).await;
    pending(review.as_mut()).await;
    assert_eq!(clone.authority_domain.snapshot(), Snapshot { active: 0, queued: 3, uncertain: false, writers_stopped: false });
    assert_eq!(store.find("corp", "alice").await.unwrap(), Some(original.clone()));
    drop(link);
    drop(unlink);
    assert_eq!(fixture.db.authority_domain.snapshot().queued, 1);
    assert!(store.find("github", "bob").await.unwrap().is_none());
    assert_eq!(store.find("corp", "alice").await.unwrap(), Some(original));
    let mut link = Box::pin(store.link(fixture.bob, identity("github", "bob"), Timestamp(2)));
    let mut unlink = Box::pin(store.unlink(fixture.alice, "corp", &usable));
    pending(link.as_mut()).await;
    pending(unlink.as_mut()).await;
    drop(clone.close());
    drop(fixture.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    assert!(matches!(review.await, Err(StorageError::Conflict("package policy writer stopped"))));
    stopped(&link.await);
    stopped(&unlink.await);
    stopped(&store.link(fixture.bob, identity("github", "bob"), Timestamp(3)).await);
    stopped(&store.unlink(fixture.alice, "corp", &usable).await);
    stopped(&fixture.db.external_identities().link(fixture.bob, identity("github", "bob"), Timestamp(3)).await);
    stopped(&fixture.db.external_identities().unlink(fixture.alice, "corp", &usable).await);
    assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    fixture.db.close().await;
}

#[tokio::test]
async fn acquired_link_and_unlink_cancellation_remain_uncertain_with_real_sql_writer_held() {
    for unlink in [false, true] {
        let fixture = Fixture::new().await;
        let store = fixture.db.external_identities();
        store.link(fixture.alice, identity("corp", "alice"), Timestamp(1)).await.unwrap().unwrap();
        let mut connection = fixture.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation: Pin<Box<dyn Future<Output = Result<(), StorageError>> + '_>> = if unlink {
            Box::pin(async { store.unlink(fixture.alice, "corp", &["corp"]).await.map(|_| ()) })
        } else {
            Box::pin(async { store.link(fixture.bob, identity("github", "bob"), Timestamp(2)).await.map(|_| ()) })
        };
        pending(operation.as_mut()).await;
        assert_eq!(fixture.db.authority_domain.snapshot().active, 1);
        drop(operation);
        assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(fixture.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // This proves enrollment before the first SQL await, not actual worker
        // entry or known settlement of the canceled driver operation.
        fixture.db.close().await;
    }
}

#[tokio::test]
async fn failed_link_keeps_uncertainty_without_disabling_later_passive_linking() {
    let fixture = Fixture::new().await;
    let store = fixture.db.external_identities();
    assert!(store.link(AccountId(i64::MAX), identity("corp", "missing"), Timestamp(1)).await.is_err());
    assert!(fixture.db.authority_domain.snapshot().uncertain);
    store.link(fixture.alice, identity("corp", "alice"), Timestamp(2)).await.unwrap().unwrap();
    assert_eq!(store.find("corp", "alice").await.unwrap().unwrap().account_id, fixture.alice);
    assert!(matches!(fixture.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    fixture.db.close().await;
}
