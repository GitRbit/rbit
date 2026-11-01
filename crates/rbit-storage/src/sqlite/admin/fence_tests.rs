//! Three moderation writes only; no whole-host/physical-owner coverage claim.
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
    charlie: AccountId,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = account(&db, "alice", true).await;
        let bob = account(&db, "bob", true).await;
        let charlie = account(&db, "charlie", false).await;
        Self { dir, db, alice, bob, charlie }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db")).foreign_keys(true))
            .await
            .unwrap()
    }
}
async fn account(db: &Database, name: &str, admin: bool) -> AccountId {
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
        matches!(result, Err(StorageError::AdmissionUnavailable { scope: "account moderation", reason: crate::AdmissionReason::Stopped })),
        "{result:?}"
    );
}

#[tokio::test]
async fn applied_repeated_noop_last_admin_and_missing_outcomes_commit_cleanly() {
    let fixture = Fixture::new().await;
    let store = fixture.db.admin();
    assert_eq!(store.set_admin(fixture.charlie, true).await.unwrap(), Moderation::Applied);
    assert_eq!(store.set_admin(fixture.charlie, true).await.unwrap(), Moderation::Applied);
    assert_eq!(store.set_admin(fixture.charlie, false).await.unwrap(), Moderation::Applied);
    assert_eq!(store.set_admin(fixture.bob, false).await.unwrap(), Moderation::Applied);
    assert_eq!(store.set_suspended(fixture.bob, false).await.unwrap(), Moderation::Applied);
    assert_eq!(store.set_suspended(fixture.bob, false).await.unwrap(), Moderation::Applied);
    assert_eq!(store.set_admin(fixture.alice, false).await.unwrap(), Moderation::LastAdmin);
    assert_eq!(store.set_suspended(fixture.alice, true).await.unwrap(), Moderation::LastAdmin);
    assert_eq!(store.delete_user(fixture.alice).await.unwrap(), Moderation::LastAdmin);
    let missing = AccountId(i64::MAX);
    assert_eq!(store.set_admin(missing, false).await.unwrap(), Moderation::NotFound);
    assert_eq!(store.set_suspended(missing, true).await.unwrap(), Moderation::NotFound);
    assert_eq!(store.delete_user(missing).await.unwrap(), Moderation::NotFound);
    assert_eq!(store.delete_user(fixture.charlie).await.unwrap(), Moderation::Applied);
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    fixture.db.close().await;
}

#[tokio::test]
async fn policy_and_moderation_clones_share_serial_and_stop_without_serializing_admin_reads() {
    let fixture = Fixture::new().await;
    let clone = fixture.db.clone();
    let old_admin = clone.admin();
    let policy = fixture.db.plugin_policy();
    let quiet = fixture.db.authority_domain.drain(deadline()).await.unwrap();
    assert!(old_admin.find_user(fixture.bob).await.unwrap().is_some());
    let mut moderation = Box::pin(old_admin.set_suspended(fixture.bob, true));
    pending(moderation.as_mut()).await;
    let mut policy_write = Box::pin(policy.prepare(fixture.alice, Timestamp(1)));
    pending(policy_write.as_mut()).await;
    assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 2, uncertain: false, writers_stopped: false });
    assert_eq!(clone.authority_domain.snapshot(), fixture.db.authority_domain.snapshot());
    drop(fixture.db.close()); // Terminal preclose even without polling pool closure.
    drop(fixture.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    stopped(&moderation.await);
    assert!(matches!(
        policy_write.await,
        Err(StorageError::AdmissionUnavailable { scope: "package policy", reason: crate::AdmissionReason::Stopped })
    ));
    stopped(&old_admin.set_admin(fixture.bob, false).await);
    stopped(&clone.admin().set_suspended(fixture.bob, true).await);
    stopped(&fixture.db.admin().delete_user(fixture.bob).await);
    assert!(fixture.db.accounts().find_active_user(fixture.bob).await.unwrap().is_some());
    assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    fixture.db.close().await;
}

#[tokio::test]
async fn acquired_cancellation_while_real_sql_writer_is_held_is_sticky_for_each_moderation_method() {
    for kind in 0..3 {
        let fixture = Fixture::new().await;
        let store = fixture.db.admin();
        let mut connection = fixture.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation: Pin<Box<dyn Future<Output = Result<Moderation, StorageError>> + '_>> = match kind {
            0 => Box::pin(store.set_admin(fixture.bob, false)),
            1 => Box::pin(store.set_suspended(fixture.bob, true)),
            _ => Box::pin(store.delete_user(fixture.bob)),
        };
        pending(operation.as_mut()).await;
        assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false, writers_stopped: false });
        drop(operation);
        assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(fixture.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // Registration-before-first-SQL-await, not proof of worker entry or a
        // claimed rollback/commit outcome for the canceled driver command.
        fixture.db.close().await;
    }
}

#[tokio::test]
async fn guarded_account_delete_preserves_historical_policy_actor_fk_scrubbing() {
    let fixture = Fixture::new().await;
    let policy = fixture.db.plugin_policy();
    let before = policy.prepare(fixture.bob, Timestamp(1)).await.unwrap();
    let mut connection = fixture.connection().await;
    let preparer: Option<i64> =
        sqlx::query_scalar("SELECT prepared_by FROM plugin_policy_epochs WHERE retired = 0").fetch_one(&mut connection).await.unwrap();
    assert_eq!(preparer, Some(fixture.bob.0));
    assert_eq!(fixture.db.admin().delete_user(fixture.bob).await.unwrap(), Moderation::Applied);
    let after = policy.gate(fixture.alice).await.unwrap();
    assert_eq!(after.epoch, before.epoch);
    assert_eq!(after.fingerprint, before.fingerprint);
    let preparer: Option<i64> =
        sqlx::query_scalar("SELECT prepared_by FROM plugin_policy_epochs WHERE retired = 0").fetch_one(&mut connection).await.unwrap();
    assert_eq!(preparer, None);
    assert!(fixture.db.accounts().find_active_user(fixture.bob).await.unwrap().is_none());
    crate::sqlite::plugin_policy::validate_snapshot(&mut connection).await.unwrap();
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    fixture.db.close().await;
}
