//! Passkey challenge/counter authority remains the existing owner for original proof.
use super::*;
use rbit_storage::sqlite::credential_revocations as intents;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

async fn pending_original(fixture: &Fixture) -> rbit_auth::SessionToken {
    let proven = fixture.services.accounts.authenticate_original_proof("alice", PASSWORD.into(), |_| Ok(())).await.1.unwrap();
    fixture.services.two_factor.start_original_password_pending(proven)
}
#[tokio::test]
async fn original_passkey_finish_retains_challenge_counter_single_use_and_epoch() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(14);
    fixture.register(&alice, &key, "security key").await.unwrap();
    let pending = pending_original(&fixture).await;
    let options = fixture.services.two_factor.sign_in_options(&pending).await.unwrap().unwrap();
    let answer = key.get(&options);
    let before = fixture.database.two_factor().passkeys(alice.id()).await.unwrap().remove(0).sign_count;
    assert!(matches!(
        fixture.services.complete_sign_in(&pending, SecondFactorProof::Passkey(&answer), &client()).await,
        Err(ServiceError::NotFound)
    ));
    assert_eq!(fixture.database.two_factor().passkeys(alice.id()).await.unwrap().remove(0).sign_count, before);
    let result = fixture.services.complete_original_sign_in(&pending, SecondFactorProof::Passkey(&answer), &client()).await.unwrap();
    assert_eq!(result.receipt().original_epoch().get(), 0);
    assert_eq!(result.user().id(), alice.id());
    assert_eq!(fixture.database.two_factor().passkeys(alice.id()).await.unwrap().remove(0).sign_count, before + 1);
    assert!(matches!(
        fixture.services.complete_original_sign_in(&pending, SecondFactorProof::Passkey(&answer), &client()).await,
        Err(ServiceError::NotFound)
    ));
}
#[tokio::test]
async fn stale_original_passkey_rejects_before_counter_consumption() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(15);
    fixture.register(&alice, &key, "security key").await.unwrap();
    let pending = pending_original(&fixture).await;
    let options = fixture.services.two_factor.sign_in_options(&pending).await.unwrap().unwrap();
    let answer = key.get(&options);
    let before = fixture.database.two_factor().passkeys(alice.id()).await.unwrap().remove(0).sign_count;
    let mut config = Config::default();
    config.storage.data_dir = fixture.directory.path().to_path_buf();
    let raw = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new())
                .filename(config.storage.sqlite_path())
                .foreign_keys(true),
        )
        .await
        .unwrap();
    let mut tx = raw.begin().await.unwrap();
    intents::stage(&mut tx, alice.id(), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        fixture.services.complete_original_sign_in(&pending, SecondFactorProof::Passkey(&answer), &client()).await,
        Err(ServiceError::NotFound)
    ));
    assert_eq!(fixture.database.two_factor().passkeys(alice.id()).await.unwrap().remove(0).sign_count, before);
    raw.close().await;
}
