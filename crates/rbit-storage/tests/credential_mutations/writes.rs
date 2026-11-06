use super::fixture::*;
use rbit_core::Timestamp;
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::{
    Work, count_work,
    sqlite::{credential_epochs::allocate_in, credential_revocations as intents},
};

#[tokio::test]
async fn self_service_commits_one_fixed_global_intent_with_password_and_link_revocation() {
    let fixture = Fixture::new().await;
    let (result, work) = count_work(fixture.self_change(OLD, CredentialEpoch::INITIAL)).await;
    let intent = result.unwrap().unwrap();
    assert_eq!(intent.account_id, fixture.account);
    assert_eq!(intent.target_epoch, 1);
    assert_eq!(intent.created_at, NOW);
    assert_eq!(work, Work { acquisitions: 1, statements: 4, cache_writes: 0 });
    let state = fixture.state().await;
    assert_eq!(state.login, (NEW.into(), 1, 0, None));
    assert_eq!(state.high_water, 1);
    assert_eq!(state.intents, [(fixture.account.0, 1, None)]);
    assert_eq!(state.links.iter().map(|link| link.2).collect::<Vec<_>>(), [Some(100), Some(100), None]);
    fixture.raw.close().await;
    fixture.database.close().await;
    let database = rbit_storage::Database::open(&fixture.directory.path().join("credential.db")).await.unwrap();
    assert_eq!(database.accounts().find_login("alice").await.unwrap().unwrap().password_hash, NEW);
    let snapshot = database.accounts().find_login_snapshot("alice").await.unwrap().unwrap();
    assert_eq!(snapshot.fence.epoch.get(), 1);
    assert!(snapshot.fence.pending, "commit is not a cleanup acknowledgment");
    database.close().await;
}

#[tokio::test]
async fn failed_original_cas_suspension_and_pending_never_commit_allocation_or_revoke_links() {
    for case in ["hash", "epoch", "suspended", "pending"] {
        let fixture = Fixture::new().await;
        if case == "suspended" {
            fixture.execute("UPDATE user_logins SET suspended_at=50").await;
        }
        if case == "pending" {
            // Current-target pending is synthetic INITIAL here; SQL schema forbids
            // target0, so use a globally allocated epoch1 and matching pending intent.
            fixture.self_change(OLD, CredentialEpoch::INITIAL).await.unwrap().unwrap();
        }
        let before = fixture.state().await;
        let epoch = CredentialEpoch::new(if case == "pending" {
            1
        } else if case == "epoch" {
            9
        } else {
            0
        })
        .unwrap();
        let result = fixture
            .self_change(
                if case == "hash" {
                    "stale PHC"
                } else if case == "pending" {
                    NEW
                } else {
                    OLD
                },
                epoch,
            )
            .await;
        if case == "epoch" {
            assert!(result.is_err(), "allocator below accepted original refuses");
        } else {
            assert!(result.unwrap().is_none(), "{case}");
        }
        assert_eq!(fixture.state().await, before, "{case}: complete rollback");
    }
}

#[tokio::test]
async fn administrator_supersedes_pending_at_fixed_target_and_cannot_follow_recreated_account() {
    let fixture = Fixture::new().await;
    let first = fixture.self_change(OLD, CredentialEpoch::INITIAL).await.unwrap().unwrap();
    let replacement = fixture
        .database
        .profiles()
        .set_password_hash_with_intent(fixture.account, "admin PHC", CredentialEpoch::new(1).unwrap(), NOW)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replacement.target_epoch, 2);
    let mut tx = fixture.raw.begin().await.unwrap();
    assert_eq!(
        intents::complete(&mut tx, &first, intents::RevocationOutcome::EndedAll, NOW).await.unwrap(),
        intents::Completion::NotCurrent
    );
    tx.commit().await.unwrap();
    assert_eq!(fixture.state().await.intents, [(fixture.account.0, 1, None), (fixture.account.0, 2, None)]);
    // Required future creation protocol: allocate in the SAME recreation transaction.
    let mut tx = fixture.raw.begin().await.unwrap();
    sqlx::query("DELETE FROM accounts WHERE id=?").bind(fixture.account.0).execute(&mut *tx).await.unwrap();
    let created = allocate_in(&mut tx).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(?,'user','replacement',10,10)")
        .bind(fixture.account.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_logins(account_id,email,password_hash,authentication_epoch) VALUES(?,'replacement@example.com','new account',?)",
    )
    .bind(fixture.account.0)
    .bind(created.get())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let before = fixture.state().await;
    assert!(
        fixture
            .database
            .profiles()
            .set_password_hash_with_intent(fixture.account, "stale administrator target", CredentialEpoch::new(2).unwrap(), NOW)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(fixture.state().await, before);
}

#[tokio::test]
async fn reset_links_have_one_winner_and_one_atomic_intent_without_an_extra_account_read() {
    let fixture = Fixture::new().await;
    let (result, work) = count_work(fixture.reset()).await;
    let mutation = result.unwrap().unwrap();
    assert_eq!(mutation.record.account_id, fixture.account);
    assert_eq!(mutation.intent.target_epoch, 1);
    assert_eq!(work, Work { acquisitions: 1, statements: 5, cache_writes: 0 });
    assert_eq!(fixture.state().await.login, (NEW.into(), 1, 0, Some(100)));
    assert!(fixture.database.recovery().reset_password_with_intent(&[2; 32], "loser", NOW).await.unwrap().is_none());
    let concurrent = Fixture::new().await;
    let store = concurrent.database.recovery();
    let (first, second) = tokio::join!(
        store.reset_password_with_intent(&[1; 32], "winner one", NOW),
        store.reset_password_with_intent(&[2; 32], "winner two", NOW)
    );
    assert_eq!(usize::from(first.unwrap().is_some()) + usize::from(second.unwrap().is_some()), 1);
    assert_eq!(concurrent.state().await.high_water, 1);
    assert_eq!(concurrent.state().await.intents.len(), 1);
}

#[tokio::test]
async fn wrong_expired_suspended_and_changed_email_reset_proofs_do_not_allocate_or_consume() {
    for case in ["purpose", "expired", "suspended", "email"] {
        let fixture = Fixture::new().await;
        match case {
            "suspended" => fixture.execute("UPDATE user_logins SET suspended_at=50").await,
            "email" => fixture.execute("UPDATE user_logins SET email='changed@example.com'").await,
            _ => {}
        }
        let before = fixture.state().await;
        let digest = if case == "purpose" { [3; 32] } else { [1; 32] };
        let now = if case == "expired" { Timestamp(1001) } else { NOW };
        let (result, work) = count_work(fixture.database.recovery().reset_password_with_intent(&digest, NEW, now)).await;
        assert!(result.unwrap().is_none(), "{case}");
        assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
        assert_eq!(fixture.state().await, before);
    }
}

#[tokio::test]
async fn original_epoch_refuses_restored_phc_even_when_its_hash_matches_again() {
    let fixture = Fixture::new().await;
    fixture.self_change(OLD, CredentialEpoch::INITIAL).await.unwrap().unwrap();
    let restored = fixture
        .database
        .profiles()
        .set_password_hash_with_intent(fixture.account, OLD, CredentialEpoch::new(1).unwrap(), NOW)
        .await
        .unwrap()
        .unwrap();
    // This SQL-only fixture has never issued a session. Completing its current
    // intent does not leave pending as the reason this stale original is refused.
    let mut tx = fixture.raw.begin().await.unwrap();
    assert_eq!(
        intents::complete(&mut tx, &restored, intents::RevocationOutcome::EndedAll, NOW).await.unwrap(),
        intents::Completion::Applied
    );
    tx.commit().await.unwrap();
    let before = fixture.state().await;
    assert_eq!(before.login.0, OLD);
    assert_eq!(before.login.1, 2);
    assert!(fixture.self_change(OLD, CredentialEpoch::INITIAL).await.unwrap().is_none());
    assert_eq!(fixture.state().await, before, "a matching PHC does not restore an older epoch");
}

#[tokio::test]
async fn legacy_reset_keeps_original_sql_work_and_does_not_select_global_mode() {
    let fixture = Fixture::new().await;
    let (result, work) = count_work(fixture.database.recovery().reset_password(&[1; 32], NEW, NOW)).await;
    assert_eq!(result.unwrap().unwrap().account_id, fixture.account);
    assert_eq!(work, Work { acquisitions: 1, statements: 3, cache_writes: 0 });
    let state = fixture.state().await;
    assert_eq!(state.login, (NEW.into(), 0, 0, Some(100)));
    assert_eq!(state.high_water, 0);
    assert_eq!(state.intents.len(), 0);
}

#[tokio::test]
async fn legacy_profile_success_keeps_both_original_statements_and_failed_cas_keeps_one() {
    let fixture = Fixture::new().await;
    let store = fixture.database.profiles();
    let (result, work) = count_work(store.set_password_hash_if_current(fixture.account, NEW, OLD)).await;
    assert!(result.unwrap());
    assert_eq!(work, Work { acquisitions: 1, statements: 2, cache_writes: 0 });
    let before = fixture.state().await;
    assert_eq!(before.login.1, 0);
    assert_eq!(before.high_water, 0);
    let (result, work) = count_work(store.set_password_hash_if_current(fixture.account, "loser", OLD)).await;
    assert!(!result.unwrap());
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(fixture.state().await, before);
}
