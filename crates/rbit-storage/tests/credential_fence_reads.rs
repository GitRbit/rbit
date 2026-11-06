//! Alternate fenced reads stay one-query and do not activate existing routes.
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::{CredentialEpoch, CredentialLoginSnapshot};
use rbit_storage::sqlite::admin::Moderation;
use rbit_storage::{
    Database, Work, count_work,
    sqlite::credential_revocations::{self as intents, RevocationOutcome},
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    raw: SqlitePool,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fenced.db");
        let database = Database::open(&path).await.unwrap();
        let raw = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&path).foreign_keys(true))
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','alice',10,10),(2,'user','bob',10,10); \
             INSERT INTO user_logins(account_id,email,password_hash,is_admin,password_change_required) \
             VALUES(1,'alice@example.com','secret PHC fixture',1,1),(2,'bob@example.com','bob PHC fixture',0,0); \
             INSERT INTO repositories(id,owner_id,name,visibility,default_branch,created_at,updated_at) VALUES(1,1,'project','public','main',10,10); \
             INSERT INTO issues(id,repo_id,number,kind,title,author_id,created_at,updated_at) VALUES(1,1,1,'issue','Example',2,10,10); \
             INSERT INTO notifications(user_id,issue_id,repo_id,reason,unread,updated_at) VALUES(1,1,1,'mention',1,10);"
        ).execute(&raw).await.unwrap();
        Self { _dir: dir, database, raw }
    }
    async fn snapshot(&self, login: &str) -> Option<CredentialLoginSnapshot> {
        let (result, work) = count_work(self.database.accounts().find_login_snapshot(login)).await;
        assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
        result.unwrap()
    }
    async fn accepts(&self, account: AccountId, epoch: CredentialEpoch) -> bool {
        let (result, work) = count_work(self.database.accounts().find_session_user_at_epoch(account, true, epoch)).await;
        assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
        result.unwrap().is_some()
    }
}

#[tokio::test]
async fn snapshots_and_fenced_session_flags_are_one_statement_and_one_acquisition() {
    let fixture = Fixture::new().await;
    let snapshot = fixture.snapshot("alice@example.com").await.unwrap();
    assert_eq!(snapshot.login.user.id(), AccountId(1));
    assert_eq!(snapshot.login.password_hash, "secret PHC fixture");
    assert_eq!(snapshot.fence.epoch, CredentialEpoch::INITIAL);
    assert!(!snapshot.fence.pending);
    assert!(!format!("{snapshot:?}").contains("secret PHC fixture"));
    assert!(fixture.snapshot("missing").await.is_none());
    assert!(fixture.snapshot("' OR 1=1 --").await.is_none());
    let counter = crate::query_count::QueryCounter::new();
    let accounts = fixture.database.accounts();
    let (result, work) = count_work(counter.measure(accounts.find_session_user_at_epoch(AccountId(1), true, snapshot.fence.epoch))).await;
    assert_eq!(counter.queries().len(), 1);
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    let session = result.unwrap().unwrap();
    assert_eq!(session.user.id(), snapshot.login.user.id());
    assert_eq!(session.user.email, snapshot.login.user.email);
    assert!(session.password_change_required);
    assert!(session.has_unread_notifications);
    let disabled = accounts.find_session_user_at_epoch(AccountId(1), false, snapshot.fence.epoch).await.unwrap().unwrap();
    assert!(!disabled.has_unread_notifications);
}

#[tokio::test]
async fn original_proof_epoch_cannot_be_rehabilitated_by_reset_then_late_issuance() {
    let fixture = Fixture::new().await;
    let old = fixture.snapshot("alice").await.unwrap();
    let mut tx = fixture.raw.begin().await.unwrap();
    sqlx::query("UPDATE user_logins SET password_hash='new PHC fixture' WHERE account_id=1").execute(&mut *tx).await.unwrap();
    let reset = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    let current = fixture.snapshot("alice").await.unwrap();
    assert_eq!(current.login.password_hash, "new PHC fixture");
    assert_eq!(current.fence.epoch.get(), 1);
    assert!(current.fence.pending);
    assert!(!fixture.accepts(AccountId(1), old.fence.epoch).await);
    assert!(!fixture.accepts(AccountId(1), current.fence.epoch).await);
    let mut tx = fixture.raw.begin().await.unwrap();
    intents::complete(&mut tx, &reset, RevocationOutcome::EndedAll, Timestamp(21)).await.unwrap();
    tx.commit().await.unwrap();
    let finalized = fixture.snapshot("alice").await.unwrap();
    assert!(!finalized.fence.pending);
    assert!(!finalized.fence.accepts(old.fence.epoch));
    assert!(!fixture.accepts(AccountId(1), old.fence.epoch).await);
    assert!(fixture.accepts(AccountId(1), finalized.fence.epoch).await);
    // No redb session is minted here: the future issuing application must retain old.epoch,
    // never replace it with finalized.epoch after verification of old.password_hash.
}

#[tokio::test]
async fn old_pending_history_does_not_block_new_completed_epoch_or_hide_a_new_barrier() {
    let fixture = Fixture::new().await;
    let mut tx = fixture.raw.begin().await.unwrap();
    let old = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    let latest = intents::stage(&mut tx, AccountId(1), Timestamp(21)).await.unwrap();
    intents::complete(&mut tx, &latest, RevocationOutcome::EndedAll, Timestamp(22)).await.unwrap();
    tx.commit().await.unwrap();
    let snapshot = fixture.snapshot("alice").await.unwrap();
    assert_eq!(snapshot.fence.epoch.get(), latest.target_epoch);
    assert!(!snapshot.fence.pending);
    assert!(fixture.accepts(AccountId(1), snapshot.fence.epoch).await);
    let old_pending: bool = sqlx::query_scalar("SELECT completed_at IS NULL FROM credential_revocations WHERE id=?")
        .bind(old.id)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    assert!(old_pending);
    let mut tx = fixture.raw.begin().await.unwrap();
    intents::stage(&mut tx, AccountId(1), Timestamp(23)).await.unwrap();
    tx.commit().await.unwrap();
    let newest = fixture.snapshot("alice").await.unwrap();
    assert!(newest.fence.pending);
    assert!(!fixture.accepts(AccountId(1), snapshot.fence.epoch).await);
    assert!(!fixture.accepts(AccountId(1), newest.fence.epoch).await);
}

#[tokio::test]
async fn rolled_back_changes_preserve_legacy_zero_and_suspension_remains_post_lookup() {
    let fixture = Fixture::new().await;
    let mut tx = fixture.raw.begin().await.unwrap();
    intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    tx.rollback().await.unwrap();
    let snapshot = fixture.snapshot("alice").await.unwrap();
    assert_eq!(snapshot.fence.epoch, CredentialEpoch::INITIAL);
    assert!(!snapshot.fence.pending);
    assert!(fixture.accepts(AccountId(1), CredentialEpoch::INITIAL).await);
    assert!(matches!(fixture.database.admin().set_admin(AccountId(2), true).await.unwrap(), Moderation::Applied));
    assert!(matches!(fixture.database.admin().set_suspended(AccountId(1), true).await.unwrap(), Moderation::Applied));
    let suspended = fixture.snapshot("alice").await.unwrap();
    assert!(suspended.login.suspended, "same candidate remains available for covering/real verification order");
    assert_eq!(suspended.login.password_hash, "secret PHC fixture");
    assert!(!fixture.accepts(AccountId(1), CredentialEpoch::INITIAL).await);
    assert!(!fixture.accepts(AccountId(999), CredentialEpoch::INITIAL).await);
    assert!(fixture.database.accounts().find_login("alice").await.unwrap().is_some(), "legacy lookup remains unchanged");
}
