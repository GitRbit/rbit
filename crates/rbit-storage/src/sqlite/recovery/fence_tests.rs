//! SQL-only recovery enrollment; no later KV/session settlement certificate.
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
    fn token(&self, digest: u8, purpose: TokenPurpose) -> NewEmailToken {
        NewEmailToken {
            digest: [digest; 32],
            account_id: self.alice,
            purpose,
            email: self.email.clone(),
            expires_at: Timestamp(Timestamp::now().seconds() + 3600),
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
    assert!(matches!(result, Err(StorageError::Conflict("recovery writer stopped"))), "{result:?}");
}
async fn mutation(store: &RecoveryStore, f: &Fixture, kind: usize) -> Result<(), StorageError> {
    match kind {
        0 => store.insert(&f.token(99, TokenPurpose::Reset)).await,
        1 => store.consume(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.map(|_| ()),
        2 => store.reset_password(&[1; 32], "new-hash", Timestamp::now()).await.map(|_| ()),
        3 => store.revoke_all(f.alice, TokenPurpose::Reset).await.map(|_| ()),
        _ => store.mark_verified(f.alice, &f.email, Timestamp::now()).await.map(|_| ()),
    }
}

#[tokio::test]
async fn consume_preserves_purpose_expiry_email_and_one_use_with_clean_noops() {
    let f = Fixture::new().await;
    let store = f.db.recovery();
    let token = f.token(1, TokenPurpose::Reset);
    store.insert(&token).await.unwrap();
    assert!(store.consume(&token.digest, TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_none());
    assert!(store.consume(&token.digest, TokenPurpose::Reset, token.expires_at).await.unwrap().is_none());
    let mut wrong_email = f.token(2, TokenPurpose::Verify);
    wrong_email.email = "former@example.com".parse().unwrap();
    store.insert(&wrong_email).await.unwrap();
    assert!(store.consume(&wrong_email.digest, TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_none());
    let record = store.consume(&token.digest, TokenPurpose::Reset, Timestamp::now()).await.unwrap().unwrap();
    assert_eq!(record.account_id, f.alice);
    assert_eq!(record.email, f.email);
    assert!(store.consume(&token.digest, TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_none());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn reset_success_and_suspended_missing_email_changed_noops_all_settle() {
    let f = Fixture::new().await;
    let store = f.db.recovery();
    for (id, purpose) in [(1, TokenPurpose::Reset), (2, TokenPurpose::Reset), (3, TokenPurpose::Verify)] {
        store.insert(&f.token(id, purpose)).await.unwrap();
    }
    assert!(store.reset_password(&[9; 32], "ignored", Timestamp::now()).await.unwrap().is_none());
    f.db.admin().set_suspended(f.alice, true).await.unwrap();
    assert!(store.reset_password(&[1; 32], "ignored", Timestamp::now()).await.unwrap().is_none());
    f.db.admin().set_suspended(f.alice, false).await.unwrap();
    let mut stale = f.token(4, TokenPurpose::Reset);
    stale.email = "former@example.com".parse().unwrap();
    store.insert(&stale).await.unwrap();
    assert!(store.reset_password(&stale.digest, "ignored", Timestamp::now()).await.unwrap().is_none());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    let now = Timestamp::now();
    assert_eq!(store.reset_password(&[1; 32], "new-hash", now).await.unwrap().unwrap().account_id, f.alice);
    assert!(!f.db.accounts().password_change_required(f.alice).await.unwrap());
    assert_eq!(f.db.accounts().find_login("alice").await.unwrap().unwrap().password_hash, "new-hash");
    assert_eq!(store.find_account(f.alice).await.unwrap().unwrap().email_verified_at, Some(now));
    assert!(store.find_valid(&[2; 32], TokenPurpose::Reset, now).await.unwrap().is_none());
    assert!(store.find_valid(&[3; 32], TokenPurpose::Verify, now).await.unwrap().is_some());
    assert!(store.reset_password(&[1; 32], "ignored", now).await.unwrap().is_none());
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn revocation_and_verified_email_match_scope_and_zero_false_are_clean() {
    let f = Fixture::new().await;
    let store = f.db.recovery();
    store.insert(&f.token(1, TokenPurpose::Reset)).await.unwrap();
    store.insert(&f.token(2, TokenPurpose::Verify)).await.unwrap();
    let mut bob = f.token(3, TokenPurpose::Reset);
    bob.account_id = f.bob;
    bob.email = "bob@example.com".parse().unwrap();
    store.insert(&bob).await.unwrap();
    assert_eq!(store.revoke_all(f.alice, TokenPurpose::Reset).await.unwrap(), 1);
    assert_eq!(store.revoke_all(f.alice, TokenPurpose::Reset).await.unwrap(), 0);
    assert!(store.find_valid(&[2; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_some());
    assert!(store.find_valid(&[3; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_some());
    let wrong: Email = "former@example.com".parse().unwrap();
    assert!(!store.mark_verified(f.alice, &wrong, Timestamp(10)).await.unwrap());
    assert!(!store.mark_verified(AccountId(i64::MAX), &f.email, Timestamp(10)).await.unwrap());
    assert!(store.mark_verified(f.alice, &f.email, Timestamp(10)).await.unwrap());
    assert_eq!(store.find_account(f.alice).await.unwrap().unwrap().email_verified_at, Some(Timestamp(10)));
    drop(f.db.authority_domain.drain(deadline()).await.unwrap());
    f.db.close().await;
}

#[tokio::test]
async fn insert_failure_rolls_back_expired_sweep_and_later_success_can_proceed() {
    let f = Fixture::new().await;
    let store = f.db.recovery();
    let mut expired = f.token(1, TokenPurpose::Reset);
    expired.expires_at = Timestamp(1);
    store.insert(&f.token(2, TokenPurpose::Reset)).await.unwrap();
    store.insert(&expired).await.unwrap();
    assert!(store.insert(&f.token(2, TokenPurpose::Reset)).await.is_err());
    let mut connection = f.connection().await;
    let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_tokens WHERE digest = ?")
        .bind(expired.digest.as_slice())
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(retained, 1, "failed insert cannot commit its expired-token sweep");
    assert!(f.db.authority_domain.snapshot().uncertain);
    store.insert(&f.token(3, TokenPurpose::Verify)).await.unwrap();
    let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_tokens WHERE digest = ?")
        .bind(expired.digest.as_slice())
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(retained, 0);
    assert!(matches!(f.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    f.db.close().await;
}

#[tokio::test]
async fn recovery_five_paths_share_policy_queue_and_terminal_old_fresh_handle_stop() {
    let f = Fixture::new().await;
    let clone = f.db.clone();
    let store = clone.recovery().clone();
    store.insert(&f.token(1, TokenPurpose::Reset)).await.unwrap();
    let policy = f.db.plugin_policy();
    let quiet = f.db.authority_domain.drain(deadline()).await.unwrap();
    let mut operations: Vec<_> = (0..5).map(|kind| Box::pin(mutation(&store, &f, kind))).collect();
    for operation in &mut operations {
        pending(operation.as_mut()).await;
    }
    let mut review = Box::pin(policy.prepare(f.alice, Timestamp(1)));
    pending(review.as_mut()).await;
    assert_eq!(clone.authority_domain.snapshot(), Snapshot { active: 0, queued: 6, uncertain: false, writers_stopped: false });
    assert!(store.find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_some());
    drop(operations.pop().unwrap());
    assert_eq!(f.db.authority_domain.snapshot().queued, 5);
    assert_eq!(store.find_account(f.alice).await.unwrap().unwrap().email_verified_at, None);
    drop(clone.close());
    drop(f.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    for operation in operations {
        stopped(&operation.await);
    }
    assert!(matches!(review.await, Err(StorageError::Conflict("package policy writer stopped"))));
    for kind in 0..5 {
        stopped(&mutation(&store, &f, kind).await);
        stopped(&mutation(&f.db.recovery(), &f, kind).await);
    }
    assert!(store.find_valid(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_some());
    assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    f.db.close().await;
}

#[tokio::test]
async fn acquired_recovery_cancellation_is_uncertain_for_each_sql_mutator() {
    for kind in 0..5 {
        let f = Fixture::new().await;
        let store = f.db.recovery();
        store.insert(&f.token(1, TokenPurpose::Reset)).await.unwrap();
        let mut connection = f.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation = Box::pin(mutation(&store, &f, kind));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1);
        drop(operation);
        assert_eq!(f.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(f.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // Registration before the first SQL await only, not worker-entry or a
        // known commit/rollback outcome of the canceled driver operation.
        f.db.close().await;
    }
}
