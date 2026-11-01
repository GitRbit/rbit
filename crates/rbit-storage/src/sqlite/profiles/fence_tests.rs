//! Four credential SQL APIs only; metadata, rename and later KV work are separate.
use super::*;
use crate::{
    Database, NewUser,
    sqlite::recovery::{NewEmailToken, TokenPurpose},
};
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
    email: Email,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&db, "alice").await;
        let bob = user(&db, "bob").await;
        Self { dir, db, alice, bob, email: "alice@example.com".parse().unwrap() }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db"))).await.unwrap()
    }
    async fn links(&self) {
        for (digest, purpose) in [(1, TokenPurpose::Reset), (2, TokenPurpose::Verify)] {
            self.db
                .recovery()
                .insert(&NewEmailToken {
                    digest: [digest; 32],
                    account_id: self.alice,
                    purpose,
                    email: self.email.clone(),
                    expires_at: Timestamp(Timestamp::now().seconds() + 3600),
                })
                .await
                .unwrap();
        }
    }
}
async fn user(db: &Database, name: &str) -> AccountId {
    db.accounts()
        .create_user_with_initial_password(NewUser {
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
    assert!(matches!(result, Err(StorageError::Conflict("profile credential writer stopped"))), "{result:?}");
}
async fn mutation(store: &ProfileStore, f: &Fixture, kind: usize) -> Result<(), StorageError> {
    let email: Email = "new@example.com".parse().unwrap();
    match kind {
        0 => store.set_email(f.alice, &email).await,
        1 => store.set_email_if_password(f.alice, &email, "old-hash").await.map(|_| ()),
        2 => store.set_password_hash(f.alice, "new-hash").await,
        _ => store.set_password_hash_if_current(f.alice, "new-hash", "old-hash").await.map(|_| ()),
    }
}

#[tokio::test]
async fn password_checked_email_change_preserves_verification_and_invalidates_old_links() {
    let f = Fixture::new().await;
    let store = f.db.profiles();
    f.links().await;
    f.db.recovery().mark_verified(f.alice, &f.email, Timestamp(10)).await.unwrap();
    let new: Email = "new@example.com".parse().unwrap();
    assert!(!store.set_email_if_password(f.alice, &new, "stale").await.unwrap());
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_some());
    assert!(f.db.recovery().find_valid(&[2; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_some());
    let same: Email = "ALICE@example.com".parse().unwrap();
    store.set_email(f.alice, &same).await.unwrap();
    assert_eq!(f.db.recovery().find_account(f.alice).await.unwrap().unwrap().email_verified_at, Some(Timestamp(10)));
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_some());
    assert!(f.db.recovery().find_valid(&[2; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_some());
    assert!(store.set_email_if_password(f.alice, &new, "old-hash").await.unwrap());
    let account = f.db.recovery().find_account(f.alice).await.unwrap().unwrap();
    assert_eq!(account.email, new);
    assert_eq!(account.email_verified_at, None);
    store.set_email(f.alice, &f.email).await.unwrap();
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_none());
    assert!(f.db.recovery().find_valid(&[2; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_none());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn password_predicate_and_missing_account_noops_are_clean_and_success_revokes_only_reset() {
    let f = Fixture::new().await;
    let store = f.db.profiles();
    f.links().await;
    assert!(!store.set_password_hash_if_current(f.alice, "ignored", "stale").await.unwrap());
    assert_eq!(store.password_hash(f.alice).await.unwrap().as_deref(), Some("old-hash"));
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_some());
    let missing = AccountId(i64::MAX);
    store.set_email(missing, &f.email).await.unwrap();
    assert!(!store.set_email_if_password(missing, &f.email, "old-hash").await.unwrap());
    store.set_password_hash(missing, "ignored").await.unwrap();
    assert!(!store.set_password_hash_if_current(missing, "ignored", "old-hash").await.unwrap());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    assert!(store.set_password_hash_if_current(f.alice, "new-hash", "old-hash").await.unwrap());
    assert!(!f.db.accounts().password_change_required(f.alice).await.unwrap());
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_none());
    assert!(f.db.recovery().find_valid(&[2; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_some());
    assert!(!store.set_password_hash_if_current(f.alice, "ignored", "old-hash").await.unwrap());
    store.set_password_hash(f.alice, "last-hash").await.unwrap();
    assert_eq!(store.password_hash(f.alice).await.unwrap().as_deref(), Some("last-hash"));
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn conflicting_email_rolls_back_token_revocation_and_allows_later_passive_change() {
    let f = Fixture::new().await;
    let store = f.db.profiles();
    f.links().await;
    f.db.recovery().mark_verified(f.alice, &f.email, Timestamp(10)).await.unwrap();
    let taken: Email = "bob@example.com".parse().unwrap();
    assert!(matches!(store.set_email(f.alice, &taken).await, Err(StorageError::Conflict("email"))));
    let account = f.db.recovery().find_account(f.alice).await.unwrap().unwrap();
    assert_eq!(account.email, f.email);
    assert_eq!(account.email_verified_at, Some(Timestamp(10)));
    assert!(f.db.recovery().find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_some());
    assert!(f.db.recovery().find_valid(&[2; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_some());
    assert_eq!(store.password_hash(f.bob).await.unwrap().as_deref(), Some("old-hash"));
    assert!(f.db.authority_domain.snapshot().uncertain);
    store.set_password_hash(f.alice, "later-hash").await.unwrap();
    assert!(matches!(f.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    f.db.close().await;
}

#[tokio::test]
async fn four_profile_credentials_share_policy_queue_and_terminal_cloned_handle_stop() {
    let f = Fixture::new().await;
    let clone = f.db.clone();
    let store = clone.profiles().clone();
    let policy = f.db.plugin_policy();
    let quiet = f.db.authority_domain.drain(deadline()).await.unwrap();
    let mut operations: Vec<_> = (0..4).map(|kind| Box::pin(mutation(&store, &f, kind))).collect();
    for operation in &mut operations {
        pending(operation.as_mut()).await;
    }
    let mut review = Box::pin(policy.prepare(f.alice, Timestamp(1)));
    pending(review.as_mut()).await;
    assert_eq!(clone.authority_domain.snapshot(), Snapshot { active: 0, queued: 5, uncertain: false, writers_stopped: false });
    assert_eq!(store.password_hash(f.alice).await.unwrap().as_deref(), Some("old-hash"));
    drop(operations.pop().unwrap());
    assert_eq!(f.db.authority_domain.snapshot().queued, 4);
    drop(clone.close());
    drop(f.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    for operation in operations {
        stopped(&operation.await);
    }
    assert!(matches!(review.await, Err(StorageError::Conflict("package policy writer stopped"))));
    for kind in 0..4 {
        stopped(&mutation(&store, &f, kind).await);
        stopped(&mutation(&f.db.profiles(), &f, kind).await);
    }
    assert_eq!(store.password_hash(f.alice).await.unwrap().as_deref(), Some("old-hash"));
    assert_eq!(f.db.accounts().find_user(f.alice).await.unwrap().unwrap().email, f.email);
    assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    f.db.close().await;
}

#[tokio::test]
async fn all_acquired_profile_credentials_cancel_uncertain_with_real_sql_writer_held() {
    for kind in 0..4 {
        let f = Fixture::new().await;
        let store = f.db.profiles();
        let mut connection = f.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation = Box::pin(mutation(&store, &f, kind));
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
