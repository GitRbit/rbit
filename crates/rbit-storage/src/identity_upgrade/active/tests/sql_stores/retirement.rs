use super::*;
use crate::sqlite::credential_revocations::{self as intents, RevocationOutcome};
use rbit_identity_models::sessions::{PendingSessionRevocation, SessionRevocationState as State};
use std::num::NonZeroU8;

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct IntentMetadata {
    id: i64,
    account_id: i64,
    target_epoch: i64,
    created_at: i64,
    outcome: Option<String>,
    completed_at: Option<i64>,
    decision: String,
    keep_spec: Option<Vec<u8>>,
    cleanup_pair: Option<Vec<u8>>,
    superseded_by_epoch: Option<i64>,
}
struct RetirementFixture {
    fixture: Fixture,
    accepted: ActiveIdentityPair,
    database: Database,
    deleted_id: i64,
}
impl RetirementFixture {
    async fn new() -> Self {
        let fixture = Fixture::new().await;
        let mut connection = fixture.connection().await;
        commit_then_delete_highest(&mut connection, 1, 64).await;
        let highest = commit_then_delete_highest(&mut connection, 65, 128).await;
        connection.close().await.unwrap();
        let mut owner = prepared(&fixture).await;
        owner.publish_active_inner().await.unwrap();
        let mut accepted = owner.accept_active_inner().await.unwrap();
        let database = accepted.database(&fixture.sql.with_file_name(crate::sealing::KEY_FILE_NAME)).await.unwrap();
        Self { fixture, accepted, database, deleted_id: highest.id }
    }
    fn old() -> PendingSessionRevocation {
        PendingSessionRevocation::from_storage(1, AccountId(2), epoch(1), None).unwrap()
    }
    async fn old_metadata(&mut self) -> Vec<IntentMetadata> {
        sqlx::query_as("SELECT id,account_id,target_epoch,created_at,outcome,completed_at,decision,keep_spec,cleanup_pair,superseded_by_epoch FROM credential_revocations WHERE id<=? ORDER BY id")
            .bind(self.accepted.checkpoint.retired).fetch_all(&mut self.accepted.owner.connection).await.unwrap()
    }
    async fn fresh_intent(&self) -> intents::RevocationIntent {
        let original = epoch(self.accepted.checkpoint.final_floor().unwrap());
        self.database
            .profiles()
            .set_password_hash_with_intent(AccountId(2), "replacement PHC", original, Timestamp::now())
            .await
            .unwrap()
            .unwrap()
    }
    async fn close(self) -> Fixture {
        let Self { fixture, accepted, database, .. } = self;
        database.close().await;
        drop(database);
        let StoppedIdentityUpgrade { connection, db, .. } = accepted.owner;
        connection.close().await.unwrap();
        drop(db);
        fixture
    }
}

async fn commit_then_delete_highest(connection: &mut sqlx::SqliteConnection, from: i64, through: i64) -> intents::RevocationIntent {
    let mut tx = connection.begin().await.unwrap();
    let mut highest = None;
    for n in from..=through {
        highest = Some(intents::stage(&mut tx, AccountId(2), Timestamp(n)).await.unwrap());
    }
    let highest = highest.unwrap();
    assert_eq!(
        intents::complete(&mut tx, &highest, RevocationOutcome::EndedAll, Timestamp(through + 1)).await.unwrap(),
        intents::Completion::Applied
    );
    tx.commit().await.unwrap();
    // The highest row was COMMITTED before its separate legitimate cleanup.
    // Never reset sqlite_sequence or confuse a rolled-back allocation with receipt.
    sqlx::query("DELETE FROM credential_revocations WHERE id=?").bind(highest.id).execute(connection).await.unwrap();
    highest
}

#[tokio::test]
async fn many_old_rows_do_not_starve_new_work_and_exact_retirement_never_restamps_history() {
    let mut f = RetirementFixture::new().await;
    let before = f.old_metadata().await;
    assert_eq!(before.len(), 126);
    let store = f.database.credential_revocations();
    let (state, work) = crate::count_work(store.state(&RetirementFixture::old())).await;
    assert_eq!(state.unwrap(), State::Retired);
    assert_eq!((work.acquisitions, work.statements), (1, 1), "one checked immutable-fact projection replaces the old state lookup");
    let fresh = f.fresh_intent().await;
    assert!(fresh.id > f.accepted.checkpoint.retired);
    assert!(fresh.id > f.deleted_id, "committed highest AUTOINCREMENT id survives cleanup/restart");
    let (rows, work) = crate::count_work(store.pending(0, NonZeroU8::new(1).unwrap())).await;
    let rows = rows.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id(), fresh.id);
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(store.state(&rows[0]).await.unwrap(), State::CurrentPending);
    assert_eq!(f.old_metadata().await, before);
    let (result, work) = crate::count_work(store.pending(-1, NonZeroU8::new(1).unwrap())).await;
    assert!(result.is_err());
    assert_eq!(work, crate::Work::default());
    drop(store);
    let fixture = f.close().await;
    let mut accepted = fixture.owner().await.accept_active_inner().await.unwrap();
    let database = accepted.database(&fixture.sql.with_file_name(crate::sealing::KEY_FILE_NAME)).await.unwrap();
    let rows = database.credential_revocations().pending(0, NonZeroU8::new(1).unwrap()).await.unwrap();
    assert_eq!(rows[0].id(), fresh.id, "restart retains exact cutoff and sequence history");
    database.close().await;
}

#[tokio::test]
async fn missing_mismatched_or_outside_floor_facts_are_never_upgrade_retirement() {
    let f = RetirementFixture::new().await;
    let store = f.database.credential_revocations();
    let old = RetirementFixture::old();
    for fact in [
        PendingSessionRevocation::from_storage(64, AccountId(2), epoch(64), None).unwrap(),
        PendingSessionRevocation::from_storage(old.id(), AccountId(1), old.target(), None).unwrap(),
        PendingSessionRevocation::from_storage(old.id(), old.account(), epoch(2), None).unwrap(),
        PendingSessionRevocation::from_storage(
            old.id(),
            old.account(),
            old.target(),
            Some(rbit_identity_models::sessions::SessionRevocationDecision::EndAll),
        )
        .unwrap(),
        PendingSessionRevocation::from_storage(old.id(), old.account(), epoch(f.accepted.checkpoint.floor + 1), None).unwrap(),
    ] {
        assert_eq!(store.state(&fact).await.unwrap(), State::Invalid);
    }
    let fresh = f.fresh_intent().await;
    let candidate = store.find(&fresh).await.unwrap().unwrap();
    assert_eq!(store.state(&candidate).await.unwrap(), State::CurrentPending);
    assert_ne!(store.state(&candidate).await.unwrap(), State::Retired);
}

#[tokio::test]
async fn malformed_new_intent_below_the_accepted_floor_refuses_restart() {
    let mut f = RetirementFixture::new().await;
    // Explicit corruption injection, not an accepted global writer: account1 has
    // no historical target1, so constraints allow the row but protocol history must not.
    sqlx::query("INSERT INTO credential_revocations(account_id,target_epoch,created_at,decision)VALUES(1,1,1,'end_all')")
        .execute(&mut f.accepted.owner.connection)
        .await
        .unwrap();
    let fixture = f.close().await;
    assert!(fixture.owner().await.accept_active_inner().await.is_err());
    let unchanged:i64=sqlx::query_scalar("SELECT count(*) FROM credential_revocations WHERE account_id=1 AND target_epoch=1 AND completed_at IS NULL AND cleanup_pair IS NULL")
        .fetch_one(&mut fixture.connection().await).await.unwrap();
    assert_eq!(unchanged, 1, "refusal must not clear or repair malformed history");
}
