use super::*;
use crate::kv::sessions::epoch::runtime;
use crate::{EpochSessionStore, EpochSnapshot, SessionRecord, SessionStore};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::{
    credential::CredentialEpoch,
    sessions::{SessionRevocationDecision as Decision, SessionRevocationRequest},
};
mod failures;
mod witness;

struct RuntimeFixture {
    fixture: Fixture,
    accepted: ActiveIdentityPair,
    sessions: SessionStore,
}
impl RuntimeFixture {
    async fn new() -> Self {
        let fixture = Fixture::new().await;
        let mut owner = prepared(&fixture).await;
        owner.publish_active_inner().await.unwrap();
        let accepted = owner.accept_active_inner().await.unwrap();
        let sessions = accepted.session_store();
        Self { fixture, accepted, sessions }
    }
    fn epoch(&self) -> EpochSessionStore {
        self.sessions.epoch_sidecar()
    }
    fn floor(&self) -> i64 {
        runtime::read(&self.accepted.owner.db).unwrap().unwrap()
    }
    /// Private fault setup only; a caller cannot strip the mode of the public store.
    fn unfenced_for_fault(&self) -> EpochSessionStore {
        SessionStore::new(Arc::clone(&self.accepted.owner.db)).epoch_sidecar()
    }
    async fn close(self) -> Fixture {
        let Self { fixture, accepted, sessions } = self;
        drop(sessions);
        let StoppedIdentityUpgrade { connection, db, .. } = accepted.owner;
        connection.close().await.unwrap();
        drop(db);
        fixture
    }
    async fn set_epoch(&mut self, target: i64) {
        let mut tx = self.accepted.owner.connection.begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query("UPDATE credential_epoch_allocator SET high_water=? WHERE singleton=1").bind(target).execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE user_logins SET authentication_epoch=? WHERE account_id=1").bind(target).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
}
fn epoch(value: i64) -> CredentialEpoch {
    CredentialEpoch::new(value).unwrap()
}
fn record(expires_in: i64) -> SessionRecord {
    let now = Timestamp::now();
    SessionRecord::at(AccountId(1), Timestamp(now.0 + expires_in), Some("192.0.2.7".into()), "private fixture agent", now)
}
fn request(target: i64, decision: Decision) -> SessionRevocationRequest {
    SessionRevocationRequest::from_storage(1, AccountId(1), epoch(target), decision).unwrap()
}
async fn put(store: &EpochSessionStore, key: u8, original: i64) -> EpochSnapshot {
    store.put_bound([key; 32], record(3600), AccountId(1), epoch(original)).await.unwrap().unwrap()
}

#[tokio::test]
async fn accepted_mode_refuses_every_legacy_api_without_touching_records_or_witness() {
    let f = RuntimeFixture::new().await;
    let epoch = f.epoch();
    let current = put(&epoch, 1, 1).await;
    let before = f.floor();
    let ((), work) = crate::count_work(async {
        assert!(f.sessions.ensure_table().is_err());
        assert!(f.sessions.put([2; 32], record(3600)).await.is_err());
        assert!(f.sessions.renew([1; 32], Timestamp(9_999_999_999), Timestamp::now()).await.is_err());
        assert!(f.sessions.get([1; 32]).await.is_err());
        assert!(f.sessions.delete([1; 32]).await.is_err());
        assert!(f.sessions.list_for_account(AccountId(1)).await.is_err());
        assert!(f.sessions.delete_for_account(AccountId(1), None).await.is_err());
        assert!(f.sessions.purge_expired().await.is_err());
        assert!(epoch.initialize_identity_if_empty().await.is_err());
    })
    .await;
    assert_eq!(work, crate::Work::default(), "no SQL/mode query is introduced");
    assert_eq!(f.floor(), before);
    assert_eq!(epoch.get_snapshot([1; 32]).await.unwrap().unwrap().record(), current.record());
    assert!(epoch.get_snapshot([2; 32]).await.unwrap().is_none());
}

#[tokio::test]
async fn active_reads_never_return_initial_for_a_missing_binding() {
    let f = RuntimeFixture::new().await;
    // Fault-inject a raw legacy row, not a supported active writer transition.
    let legacy = SessionStore::new(Arc::clone(&f.accepted.owner.db));
    legacy.put([1; 32], record(3600)).await.unwrap();
    let store = f.epoch();
    assert!(store.get_snapshot([1; 32]).await.is_err());
    assert!(store.get_live([1; 32]).await.is_err());
    assert!(store.list_bound(AccountId(1)).await.is_err());
    assert!(store.put_bound([2; 32], record(3600), AccountId(1), CredentialEpoch::INITIAL).await.is_err());
    assert!(store.get_snapshot([2; 32]).await.unwrap().is_none());
}

#[tokio::test]
async fn normal_bound_mutations_keep_wire_and_floor_through_reopen() {
    let mut f = RuntimeFixture::new().await;
    f.set_epoch(100).await;
    let store = f.epoch();
    let (snapshot, work) = crate::count_work(put(&store, 1, 100)).await;
    assert_eq!(work, crate::Work::default());
    assert_eq!(f.floor(), 100);
    let renewed = Timestamp(snapshot.record().expires_at.0 + 100);
    assert!(store.renew(&snapshot, renewed, snapshot.record().last_seen_at).await.unwrap());
    assert_eq!(store.get_snapshot([1; 32]).await.unwrap().unwrap().record().expires_at, renewed);
    assert!(store.delete(&snapshot).await.unwrap());
    assert!(store.get_snapshot([1; 32]).await.unwrap().is_none());
    assert_eq!(f.floor(), 100);
    drop(store);
    let fixture = f.close().await;
    let accepted = fixture.owner().await.accept_active_inner().await.unwrap();
    assert_eq!(runtime::read(&accepted.owner.db).unwrap(), Some(100));
    assert_eq!(accepted.session_store().epoch_sidecar().list_bound(AccountId(1)).await.unwrap().len(), 0);
}

#[tokio::test]
async fn keep_promotion_and_logout_are_witnessed_and_protocol_bound() {
    let mut f = RuntimeFixture::new().await;
    let store = f.epoch();
    let current = put(&store, 1, 1).await;
    put(&store, 2, 1).await;
    f.set_epoch(100).await;
    let request = request(100, Decision::KeepCurrent(current.promotion_spec().unwrap()));
    let pair = store.pair_identity().await.unwrap().unwrap();
    let applied = store.apply_pending_revocation(request, pair).await.unwrap();
    assert!(applied.matches(&store, &request));
    assert!(!applied.matches(&f.unfenced_for_fault(), &request), "legacy receipt cannot prove active-protocol completion");
    assert_eq!(f.floor(), 100);
    assert!(store.get_snapshot([2; 32]).await.unwrap().is_none());
    let promoted = store.get_snapshot([1; 32]).await.unwrap().unwrap();
    assert_eq!(promoted.epoch(), epoch(100));
    assert_eq!(postcard::to_allocvec(promoted.record()).unwrap(), postcard::to_allocvec(current.record()).unwrap());
    assert!(store.logout(&current).await.unwrap());
    assert_eq!(f.floor(), 100);
}
