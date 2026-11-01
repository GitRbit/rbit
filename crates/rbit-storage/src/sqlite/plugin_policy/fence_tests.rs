//! Real SQL fixtures for enrolled policy writes only, not a physical owner broker.
use super::*;
use crate::{Database, NewUser};
use rbit_authority_fence::{DrainError, Snapshot};
use rbit_plugin_api::ExpectedPolicyRevision;
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
    admin: AccountId,
    other: AccountId,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let admin = account(&db, "alice", true).await;
        let other = account(&db, "bob", false).await;
        Self { dir, db, admin, other }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db"))).await.unwrap()
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

#[tokio::test]
async fn database_clones_enroll_in_one_domain_and_canceled_queued_call_never_starts_sql() {
    let fixture = Fixture::new().await;
    let cloned = fixture.db.clone();
    let store = cloned.plugin_policy();
    let quiet = fixture.db.policy_domain.drain(deadline()).await.unwrap();
    let mut queued = Box::pin(store.prepare(fixture.admin, Timestamp(1)));
    pending(queued.as_mut()).await;
    assert_eq!(fixture.db.policy_domain.snapshot(), Snapshot { active: 0, queued: 1, uncertain: false });
    assert_eq!(cloned.policy_domain.snapshot(), fixture.db.policy_domain.snapshot());
    drop(queued);
    assert_eq!(fixture.db.policy_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false });
    drop(quiet);
    assert_eq!(store.gate(fixture.admin).await.unwrap().state, PolicyGateState::Uninitialized);
    assert_eq!(store.prepare(fixture.admin, Timestamp(2)).await.unwrap().state, PolicyGateState::Open);
    drop(fixture.db.policy_domain.drain(deadline()).await.unwrap());
    fixture.db.close().await;
}

#[tokio::test]
async fn rejected_actor_and_later_success_preserve_passive_behavior_but_not_clean_drain() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    assert!(store.prepare(fixture.other, Timestamp(1)).await.is_err());
    assert!(fixture.db.policy_domain.snapshot().uncertain);
    assert_eq!(store.prepare(fixture.admin, Timestamp(2)).await.unwrap().state, PolicyGateState::Open);
    assert_eq!(fixture.db.policy_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true });
    assert!(matches!(fixture.db.policy_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    fixture.db.close().await;
}

#[tokio::test]
async fn prepare_renew_and_apply_cancellation_under_real_sql_writer_lock_remain_uncertain() {
    for kind in 0..3 {
        let fixture = Fixture::new().await;
        let store = fixture.db.plugin_policy();
        let mut connection = fixture.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let fingerprint = HostPolicyFingerprint::new("a".repeat(64).parse().unwrap());
        let renewal = PackagePolicyRenewalRequest::new(fingerprint.clone(), "known-seal").unwrap();
        let clear = PackageSelectionClearRequest::new("example.probe".parse().unwrap(), ExpectedPolicyRevision::Absent {}, fingerprint);
        let id = OperationId::new("known-operation").unwrap();
        let mut operation: Pin<Box<dyn Future<Output = Result<(), StorageError>> + '_>> = match kind {
            0 => Box::pin(async { store.prepare(fixture.admin, Timestamp(1)).await.map(|_| ()) }),
            1 => Box::pin(async { store.renew(fixture.admin, &id, &renewal, Timestamp(1)).await.map(|_| ()) }),
            _ => Box::pin(async { store.apply_clear(fixture.admin, &id, &clear, Timestamp(1)).await.map(|_| ()) }),
        };
        pending(operation.as_mut()).await;
        assert_eq!(fixture.db.policy_domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false });
        drop(operation);
        assert_eq!(fixture.db.policy_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(fixture.db.policy_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
        // SQLx may still settle canceled driver work; this is exactly why no
        // idle counter/pool close can certify that unknown work as clean.
        fixture.db.close().await;
    }
}
