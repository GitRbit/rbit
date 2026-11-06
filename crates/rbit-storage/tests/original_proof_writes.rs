//! Dormant write predicates/commit outcomes against actual temporary SQLite state.
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::{
    Database, NewUser, count_work,
    sqlite::two_factor::{CredentialProofWrite, NewPasskey},
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    raw: SqlitePool,
    account: rbit_core::AccountId,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proof.db");
        let database = Database::open(&path).await.unwrap();
        let user = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "fixture hash".into(),
                is_admin: false,
            })
            .await
            .unwrap();
        let raw = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new()).filename(path).foreign_keys(true),
            )
            .await
            .unwrap();
        let store = database.two_factor();
        store.begin_enrolment(user.id(), &[1; 20]).await.unwrap();
        assert_eq!(store.confirm(user.id(), &[1; 20], 0, &[[3; 32], [4; 32]]).await.unwrap(), Some(true));
        Self { _dir: dir, database, raw, account: user.id() }
    }
}

#[tokio::test]
async fn successful_claims_preserve_counted_legacy_work_and_replay_refuses_without_invalidation() {
    let fixture = Fixture::new().await;
    let store = fixture.database.two_factor();
    let epoch = CredentialEpoch::INITIAL;
    let (legacy, baseline) = count_work(store.claim_step(fixture.account, &[1; 20], 1)).await;
    assert!(legacy.unwrap());
    let (original, work) = count_work(store.claim_step_at_epoch(fixture.account, &[1; 20], 2, epoch)).await;
    assert_eq!(original.unwrap(), CredentialProofWrite::Claimed);
    assert_eq!(work, baseline);
    assert_eq!(store.claim_step_at_epoch(fixture.account, &[1; 20], 2, epoch).await.unwrap(), CredentialProofWrite::Rejected);
    let (legacy, baseline) = count_work(store.spend_recovery_code(fixture.account, &[3; 32])).await;
    assert!(legacy.unwrap());
    let (original, work) = count_work(store.spend_recovery_code_at_epoch(fixture.account, &[4; 32], epoch)).await;
    assert_eq!(original.unwrap(), CredentialProofWrite::Claimed);
    assert_eq!(work, baseline);
    assert_eq!(store.spend_recovery_code_at_epoch(fixture.account, &[4; 32], epoch).await.unwrap(), CredentialProofWrite::Rejected);
    store
        .add_passkey(
            fixture.account,
            &NewPasskey { credential_id: &[7; 20], public_key: &[8; 65], algorithm: -7, sign_count: 0, transports: &[], name: "fixture" },
            &[],
        )
        .await
        .unwrap();
    let key = store.passkeys(fixture.account).await.unwrap().remove(0);
    let (legacy, baseline) = count_work(store.use_passkey(fixture.account, key.id, 0, 1)).await;
    assert!(legacy.unwrap());
    let (original, work) = count_work(store.use_passkey_at_epoch(fixture.account, key.id, 1, 2, epoch)).await;
    assert_eq!(original.unwrap(), CredentialProofWrite::Claimed);
    assert_eq!(work, baseline);
    assert_eq!(store.use_passkey_at_epoch(fixture.account, key.id, 1, 2, epoch).await.unwrap(), CredentialProofWrite::Rejected);
    fixture.raw.close().await;
}

#[tokio::test]
async fn stale_epoch_suspension_and_current_pending_do_not_spend_recovery_codes() {
    let fixture = Fixture::new().await;
    let store = fixture.database.two_factor();
    let wrong_epoch = CredentialEpoch::new(1).unwrap();
    assert_eq!(
        store.spend_recovery_code_at_epoch(fixture.account, &[3; 32], wrong_epoch).await.unwrap(),
        CredentialProofWrite::Invalidated
    );
    sqlx::query("UPDATE user_logins SET suspended_at=10 WHERE account_id=?").bind(fixture.account.0).execute(&fixture.raw).await.unwrap();
    assert_eq!(
        store.spend_recovery_code_at_epoch(fixture.account, &[3; 32], CredentialEpoch::INITIAL).await.unwrap(),
        CredentialProofWrite::Invalidated
    );
    sqlx::query("UPDATE user_logins SET suspended_at=NULL, authentication_epoch=1 WHERE account_id=?")
        .bind(fixture.account.0)
        .execute(&fixture.raw)
        .await
        .unwrap();
    sqlx::query("INSERT INTO credential_revocations(account_id,target_epoch,created_at) VALUES(?,1,10)")
        .bind(fixture.account.0)
        .execute(&fixture.raw)
        .await
        .unwrap();
    assert_eq!(
        store.spend_recovery_code_at_epoch(fixture.account, &[3; 32], wrong_epoch).await.unwrap(),
        CredentialProofWrite::Invalidated
    );
    assert_eq!(store.remaining_recovery_codes(fixture.account).await.unwrap(), 2);
    // Older unresolved intents do not block a newer current epoch.
    sqlx::query("UPDATE user_logins SET authentication_epoch=2 WHERE account_id=?")
        .bind(fixture.account.0)
        .execute(&fixture.raw)
        .await
        .unwrap();
    assert_eq!(
        store.spend_recovery_code_at_epoch(fixture.account, &[3; 32], CredentialEpoch::new(2).unwrap()).await.unwrap(),
        CredentialProofWrite::Claimed
    );
    fixture.raw.close().await;
}

#[tokio::test]
async fn deferred_commit_failure_cannot_publish_a_successful_claim_or_consume_the_code() {
    let fixture = Fixture::new().await;
    sqlx::raw_sql(
        "CREATE TABLE proof_commit_failure(account_id INTEGER REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED) STRICT; \
        CREATE TRIGGER fail_proof_commit AFTER UPDATE OF used_at ON recovery_codes WHEN NEW.used_at IS NOT NULL BEGIN \
        INSERT INTO proof_commit_failure(account_id) VALUES(999999); END;",
    )
    .execute(&fixture.raw)
    .await
    .unwrap();
    let store = fixture.database.two_factor();
    assert!(store.spend_recovery_code_at_epoch(fixture.account, &[3; 32], CredentialEpoch::INITIAL).await.is_err());
    assert_eq!(store.remaining_recovery_codes(fixture.account).await.unwrap(), 2);
    let created: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM proof_commit_failure").fetch_one(&fixture.raw).await.unwrap();
    assert_eq!(created, 0);
    sqlx::query("DROP TRIGGER fail_proof_commit").execute(&fixture.raw).await.unwrap();
    assert_eq!(
        store.spend_recovery_code_at_epoch(fixture.account, &[3; 32], CredentialEpoch::INITIAL).await.unwrap(),
        CredentialProofWrite::Claimed
    );
    assert_eq!(store.remaining_recovery_codes(fixture.account).await.unwrap(), 1);
    fixture.raw.close().await;
}
