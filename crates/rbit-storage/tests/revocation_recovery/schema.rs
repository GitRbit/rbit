use super::fixture::*;
use rbit_core::Timestamp;
use rbit_identity_models::{
    credential::CredentialEpoch,
    sessions::{CompletedSessionRevocation, SessionRevocationAcknowledgment as Ack, SessionRevocationState},
};
use rbit_storage::{
    Database,
    sqlite::{credential_revocations as intents, writer_compatibility::register},
};
use sqlx::sqlite::SqliteConnectOptions;
use std::num::NonZeroU8;

#[tokio::test]
async fn upgrade_preserves_legacy_decisions_and_old_migrator_refuses_new_schema() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("upgrade.db");
    let pool =
        sqlx::SqlitePool::connect_with(register(SqliteConnectOptions::new().filename(&path).create_if_missing(true).foreign_keys(true)))
            .await
            .unwrap();
    sqlx::migrate!("./migrations").run_to(63, &pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','alice',1,1); INSERT INTO user_logins(account_id,email,password_hash) VALUES(1,'alice@example.com','legacy PHC');").execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let completed = intents::stage(&mut tx, rbit_core::AccountId(1), Timestamp(1)).await.unwrap();
    intents::complete(&mut tx, &completed, intents::RevocationOutcome::KeptCurrent, Timestamp(2)).await.unwrap();
    let pending = intents::stage(&mut tx, rbit_core::AccountId(1), Timestamp(3)).await.unwrap();
    tx.commit().await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let decisions: Vec<(String, Option<Vec<u8>>)> =
        sqlx::query_as("SELECT decision,keep_spec FROM credential_revocations ORDER BY id").fetch_all(&pool).await.unwrap();
    assert_eq!(decisions, [("legacy".into(), None), ("legacy".into(), None)]);
    let outcome: String =
        sqlx::query_scalar("SELECT outcome FROM credential_revocations WHERE id=?").bind(completed.id).fetch_one(&pool).await.unwrap();
    assert_eq!(outcome, "kept_current");
    let mut older = sqlx::migrate!("./migrations");
    older.migrations.to_mut().retain(|migration| migration.version <= 63);
    assert!(matches!(older.run(&pool).await, Err(sqlx::migrate::MigrateError::VersionMissing(64))));
    pool.close().await;
    let database = Database::open(&path).await.unwrap();
    let candidate = database.credential_revocations().find(&pending).await.unwrap().unwrap();
    assert!(candidate.request().is_none(), "legacy is explicit, not end-all or keep authority");
    assert_eq!(database.credential_revocations().state(&candidate).await.unwrap(), SessionRevocationState::CurrentPending);
    database.close().await;
}

#[tokio::test]
async fn strict_decision_shapes_and_immutable_identity_refuse_mutation() {
    let fixture = Fixture::new().await;
    for (decision, bytes) in
        [("keep_current", None), ("keep_current", Some(vec![0_u8; 63])), ("end_all", Some(vec![0_u8; 64])), ("future", None)]
    {
        assert!(
            sqlx::query("INSERT INTO credential_revocations(account_id,target_epoch,created_at,decision,keep_spec) VALUES(?,2,1,?,?)")
                .bind(fixture.account.0)
                .bind(decision)
                .bind(bytes)
                .execute(&fixture.raw)
                .await
                .is_err()
        );
    }
    let intent = fixture.keep_change().await;
    for sql in [
        "UPDATE credential_revocations SET decision='end_all',keep_spec=NULL",
        "UPDATE credential_revocations SET rowid=rowid+100",
        "UPDATE credential_revocations SET target_epoch=target_epoch+100",
        "UPDATE credential_revocations SET created_at=created_at+1",
    ] {
        assert!(sqlx::query(sql).execute(&fixture.raw).await.is_err(), "{sql}");
    }
    assert!(
        sqlx::query("DELETE FROM credential_revocations WHERE id=?").bind(intent.id).execute(&fixture.raw).await.is_err(),
        "no clearing unresolved authority"
    );
    let request = fixture.request(&intent).await;
    let effect = fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
    fixture.database.credential_revocations().acknowledge(&request, &fixture.sessions, &effect, Timestamp(10)).await.unwrap();
    assert!(sqlx::query("UPDATE credential_revocations SET completed_at=11").execute(&fixture.raw).await.is_err());
    assert!(sqlx::query("UPDATE credential_revocations SET outcome='ended_all'").execute(&fixture.raw).await.is_err());
    let check: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check").fetch_all(&fixture.raw).await.unwrap();
    assert_eq!(check.len(), 0);
}

#[tokio::test]
async fn supersession_requires_newer_committed_cleanup_and_retains_its_witness_until_done() {
    let fixture = Fixture::new().await;
    let older = fixture.keep_change().await;
    let newer = fixture.end_all_change(CredentialEpoch::new(older.target_epoch).unwrap()).await;
    let store = fixture.database.credential_revocations();
    let old = store.find(&older).await.unwrap().unwrap();
    let witness =
        CompletedSessionRevocation::from_storage(newer.id, fixture.account, CredentialEpoch::new(newer.target_epoch).unwrap()).unwrap();
    assert_eq!(store.supersede(&old, &witness).await.unwrap(), Ack::Changed, "pending newer intent proves no cleanup");
    assert!(
        sqlx::query("UPDATE credential_revocations SET superseded_by_epoch=? WHERE id=?")
            .bind(newer.target_epoch)
            .bind(older.id)
            .execute(&fixture.raw)
            .await
            .is_err()
    );
    let request = fixture.request(&newer).await;
    let effect = fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
    store.acknowledge(&request, &fixture.sessions, &effect, Timestamp::now()).await.unwrap();
    assert!(
        sqlx::query("DELETE FROM credential_revocations WHERE id=?").bind(newer.id).execute(&fixture.raw).await.is_err(),
        "completed current witness cannot vanish first"
    );
    let foreign =
        CompletedSessionRevocation::from_storage(newer.id, rbit_core::AccountId(fixture.account.0 + 1), witness.target()).unwrap();
    assert!(store.supersede(&old, &foreign).await.is_err());
    assert_eq!(store.supersede(&old, &witness).await.unwrap(), Ack::Applied);
    assert_eq!(store.supersede(&old, &witness).await.unwrap(), Ack::AlreadyResolved);
    assert_eq!(store.pending(0, NonZeroU8::new(10).unwrap()).await.unwrap(), Vec::new());
    sqlx::query("DELETE FROM credential_revocations WHERE id=?").bind(newer.id).execute(&fixture.raw).await.unwrap();
    assert!(
        sqlx::query("UPDATE credential_revocations SET superseded_by_epoch=NULL WHERE id=?")
            .bind(older.id)
            .execute(&fixture.raw)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn account_deletion_still_cascades_with_pending_history_and_a_retained_completed_witness() {
    let fixture = Fixture::new().await;
    let older = fixture.keep_change().await;
    let newer = fixture.end_all_change(CredentialEpoch::new(older.target_epoch).unwrap()).await;
    let request = fixture.request(&newer).await;
    let effect = fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
    fixture.database.credential_revocations().acknowledge(&request, &fixture.sessions, &effect, Timestamp::now()).await.unwrap();
    sqlx::query("DELETE FROM accounts WHERE id=?").bind(fixture.account.0).execute(&fixture.raw).await.unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM credential_revocations").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(remaining, 0);
    let high_water: i64 = sqlx::query_scalar("SELECT high_water FROM credential_epoch_allocator").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(high_water, newer.target_epoch);
}

#[tokio::test]
async fn malformed_large_promotion_is_classified_without_returning_a_large_blob_or_secret_text() {
    let fixture = Fixture::new().await;
    fixture.execute("PRAGMA ignore_check_constraints=ON; INSERT INTO credential_revocations(account_id,target_epoch,created_at,decision,keep_spec) VALUES(1,2,1,'keep_current',zeroblob(1000000)); PRAGMA ignore_check_constraints=OFF;").await;
    let error = fixture.database.credential_revocations().pending(0, NonZeroU8::new(1).unwrap()).await.unwrap_err();
    assert_eq!(error.to_string(), "stored data is invalid: credential revocation recovery state is invalid");
}

#[tokio::test]
async fn keep_decision_failure_rolls_back_password_epoch_allocator_and_intent_together() {
    let fixture = Fixture::new().await;
    fixture
        .execute("CREATE TRIGGER fixture_decision_failure BEFORE INSERT ON credential_revocations BEGIN SELECT RAISE(ABORT,'fixture'); END")
        .await;
    let result = fixture
        .database
        .profiles()
        .set_password_hash_if_current_preserving(
            fixture.account,
            "new PHC",
            "old PHC",
            fixture.original,
            Timestamp::now(),
            fixture.current.promotion_spec().unwrap(),
        )
        .await;
    assert!(result.is_err());
    let row: (String, i64, i64, i64) = sqlx::query_as("SELECT password_hash,authentication_epoch,(SELECT high_water FROM credential_epoch_allocator),(SELECT count(*) FROM credential_revocations) FROM user_logins WHERE account_id=?")
        .bind(fixture.account.0).fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(row, ("old PHC".into(), fixture.original.get(), fixture.original.get(), 0));
    assert_eq!(fixture.sessions.get_live([1; 32]).await.unwrap().unwrap().epoch(), fixture.original);
}
