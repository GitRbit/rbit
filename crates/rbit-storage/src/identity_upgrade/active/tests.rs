use super::*;
use crate::identity_upgrade::tests::fixture::{Fixture, limits};
use sqlx::Connection;

async fn prepared(fixture: &Fixture) -> StoppedIdentityUpgrade {
    let mut owner = fixture.owner().await;
    owner.prepare_inner().await.unwrap();
    owner
}
#[tokio::test]
async fn publication_restarts_without_rebasing_or_ending_new_sessions() {
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    let checkpoint = owner.publish_active_inner().await.unwrap();
    let before = f.hashes_and_epochs().await;
    // Simulate a legitimate subsequent credential mutation: restart must not insist
    // accounts still carry the retired upgrade-time contiguous sequence.
    sqlx::query("UPDATE credential_epoch_allocator SET high_water=high_water+10 WHERE singleton=1")
        .execute(&mut owner.connection)
        .await
        .unwrap();
    sqlx::query("UPDATE user_logins SET authentication_epoch=authentication_epoch+10 WHERE account_id=1")
        .execute(&mut owner.connection)
        .await
        .unwrap();
    let changed = f.hashes_and_epochs().await;
    assert_ne!(changed, before);
    let now = rbit_core::Timestamp::now();
    let sessions = crate::SessionStore::new(Arc::clone(&owner.db)).epoch_sidecar();
    sessions
        .put_bound(
            [9; 32],
            crate::SessionRecord::at(rbit_core::AccountId(1), rbit_core::Timestamp(now.0 + 3600), None, "runtime", now),
            rbit_core::AccountId(1),
            rbit_identity_models::credential::CredentialEpoch::new(changed[0].1).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    drop(sessions);
    assert!(owner.publish_active_inner().await.unwrap() == checkpoint);
    drop(owner.accept_active_inner().await.unwrap());
    let owner = f.owner().await;
    let accepted = owner.accept_active_inner().await.unwrap();
    assert!(accepted.checkpoint == checkpoint);
    assert_eq!(f.hashes_and_epochs().await, changed);
    let sessions = crate::SessionStore::new(Arc::clone(&accepted.owner.db)).epoch_sidecar();
    assert!(sessions.get_snapshot([9; 32]).await.unwrap().is_some());
}
#[tokio::test]
async fn sql_publication_cut_finishes_only_the_exact_prepared_pair() {
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    pool::bind_writer(&mut owner.connection, Arc::clone(&owner.db)).await.unwrap();
    let checkpoint = owner.validate_prepared().await.unwrap();
    sql::publish(&mut owner.connection, checkpoint).await.unwrap();
    assert!(!kv::is_active(&owner.db, checkpoint, limits()).unwrap());
    drop(owner);
    // Acceptance never repairs. Only explicit publication may finish this cut.
    assert!(f.owner().await.accept_active_inner().await.is_err());
    let mut owner = f.owner().await;
    assert!(owner.publish_active_inner().await.unwrap() == checkpoint);
    drop(owner.accept_active_inner().await.unwrap());
}
#[tokio::test]
async fn unpaired_or_modified_prepared_cut_cannot_be_adopted() {
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    pool::bind_writer(&mut owner.connection, Arc::clone(&owner.db)).await.unwrap();
    let checkpoint = owner.validate_prepared().await.unwrap();
    sql::publish(&mut owner.connection, checkpoint).await.unwrap();
    sqlx::query("UPDATE user_logins SET authentication_epoch=authentication_epoch+100 WHERE account_id=1")
        .execute(&mut owner.connection)
        .await
        .unwrap();
    assert!(owner.publish_active_inner().await.is_err());
    assert!(!kv::is_active(&owner.db, checkpoint, limits()).unwrap());
}
#[tokio::test]
async fn cached_stopped_and_default_writers_cannot_use_active_credentials() {
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    // Prepared-only capabilities could write before the v3 publication.
    sqlx::query("UPDATE accounts SET bio=? WHERE id=1").bind("before").execute(&mut owner.connection).await.unwrap();
    let mut old = f.owner_connection().await;
    sqlx::query("UPDATE accounts SET bio=? WHERE id=1").bind("cached").execute(&mut old).await.unwrap();
    owner.publish_active_inner().await.unwrap();
    assert!(sqlx::query("UPDATE accounts SET bio=? WHERE id=1").bind("stale").execute(&mut old).await.is_err());
    assert!(
        sqlx::query("UPDATE user_logins SET password_hash='unfenced' WHERE account_id=1").execute(&mut f.connection().await).await.is_err()
    );
    let value: String = sqlx::query_scalar("SELECT bio FROM accounts WHERE id=1").fetch_one(&mut old).await.unwrap();
    assert_eq!(value, "cached");
    for statement in ["DELETE FROM credential_active_protocol", "UPDATE credential_active_protocol SET singleton=1"] {
        assert!(sqlx::query(statement).execute(&mut owner.connection).await.is_err());
    }
    assert!(crate::Database::open(&f.sql).await.is_err());
    assert!(crate::KvStore::open(&f.kv).is_err());
    old.close().await.unwrap();
}
#[tokio::test]
async fn private_pool_captures_actual_native_owner_until_connection_closes() {
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    owner.publish_active_inner().await.unwrap();
    let accepted = owner.accept_active_inner().await.unwrap();
    pool::verify_pool_lifetime(accepted, &f.kv).await;
    // close() drains the SQLite worker that owns the captured KV Arc.
    redb::Database::open(&f.kv).unwrap();
}
impl Fixture {
    async fn owner_connection(&self) -> sqlx::SqliteConnection {
        sqlx::SqliteConnection::connect_with(&crate::sqlite::writer_compatibility::register_stopped(
            sqlx::sqlite::SqliteConnectOptions::new().filename(&self.sql).foreign_keys(true).pragma("trusted_schema", "OFF"),
        ))
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn foreign_checkpoint_and_callback_pair_refuse_without_repair_or_authority() {
    let a = Fixture::new().await;
    let b = Fixture::new().await;
    let mut owner = prepared(&a).await;
    let mut other = prepared(&b).await;
    let other_checkpoint = other.validate_prepared().await.unwrap();
    pool::bind_writer(&mut owner.connection, Arc::clone(&owner.db)).await.unwrap();
    assert!(sql::publish(&mut owner.connection, other_checkpoint).await.is_err());
    assert!(sql::publication(&mut owner.connection).await.unwrap().is_none());
    let checkpoint = owner.publish_active_inner().await.unwrap();
    other.publish_active_inner().await.unwrap();
    let mut connection = b.connection().await;
    assert!(pool::accept_connection(&mut connection, Arc::clone(&owner.db), checkpoint, limits()).await.is_err());
    // A failed setup never installs the active writer on the foreign connection.
    assert!(sqlx::query("UPDATE accounts SET bio='unaccepted' WHERE id=1").execute(&mut connection).await.is_err());
    assert!(kv::is_active(&owner.db, checkpoint, limits()).unwrap());
}

#[tokio::test]
async fn unknown_or_malformed_kv_publication_is_not_repaired() {
    for unknown in [false, true] {
        let f = Fixture::new().await;
        let mut owner = prepared(&f).await;
        let checkpoint = owner.publish_active_inner().await.unwrap();
        {
            let tx = owner.db.begin_write().unwrap();
            if unknown {
                tx.open_table(redb::TableDefinition::<u8, u8>::new("credential_session_future_v9")).unwrap();
            } else {
                let mut bytes = checkpoint.encode();
                bytes[8] ^= 1;
                tx.open_table(redb::TableDefinition::<u8, &[u8; 64]>::new("credential_session_active_v3"))
                    .unwrap()
                    .insert(0, &bytes)
                    .unwrap();
            }
            tx.commit().unwrap();
        }
        assert!(owner.publish_active_inner().await.is_err());
        assert!(owner.accept_active_inner().await.is_err());
    }
}

#[tokio::test]
async fn runtime_floor_rolls_back_with_transaction_and_survives_no_session_cleanup() {
    use crate::kv::sessions::epoch::runtime;
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    let checkpoint = owner.publish_active_inner().await.unwrap();
    let initial = checkpoint.final_floor().unwrap();
    {
        let tx = owner.db.begin_write().unwrap();
        runtime::advance_in(&tx, initial + 20).unwrap();
        // A failed enclosing session/cleanup transaction must roll back the witness.
        drop(tx);
    }
    assert_eq!(runtime::read(&owner.db).unwrap(), Some(initial));
    {
        let mut tx = owner.db.begin_write().unwrap();
        tx.set_durability(redb::Durability::Immediate).unwrap();
        runtime::advance_in(&tx, initial + 10).unwrap();
        runtime::advance_in(&tx, initial + 1).unwrap();
        tx.commit().unwrap();
    }
    // No session binding is needed for a committed witness to survive. This is a
    // direct transaction-helper fixture, not proof finalizers already advance it.
    assert_eq!(runtime::read(&owner.db).unwrap(), Some(initial + 10));
    assert!(owner.accept_active_inner().await.is_err(), "older SQL counter cannot adopt newer KV history");
}

#[tokio::test]
async fn missing_negative_or_exhausted_runtime_floor_refuses_without_recreation() {
    use crate::kv::sessions::epoch::runtime;
    for value in [None, Some(-1), Some(i64::MAX)] {
        let f = Fixture::new().await;
        let mut owner = prepared(&f).await;
        owner.publish_active_inner().await.unwrap();
        {
            let tx = owner.db.begin_write().unwrap();
            match value {
                None => {
                    tx.delete_table(runtime::FLOOR).unwrap();
                }
                Some(value) => {
                    tx.open_table(runtime::FLOOR).unwrap().insert(0, value).unwrap();
                }
            }
            tx.commit().unwrap();
        }
        assert!(owner.publish_active_inner().await.is_err());
        assert!(owner.accept_active_inner().await.is_err());
    }
}

#[tokio::test]
async fn runtime_floor_rejects_exhaustion_without_mutating_current_floor() {
    use crate::kv::sessions::epoch::runtime;
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    let checkpoint = owner.publish_active_inner().await.unwrap();
    {
        let tx = owner.db.begin_write().unwrap();
        for target in [-1, 0, i64::MAX] {
            assert!(runtime::advance_in(&tx, target).is_err());
        }
        tx.commit().unwrap();
    }
    assert_eq!(runtime::read(&owner.db).unwrap(), Some(checkpoint.final_floor().unwrap()));
}
