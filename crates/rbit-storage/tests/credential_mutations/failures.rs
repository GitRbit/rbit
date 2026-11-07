use super::fixture::*;
use rbit_identity_models::credential::CredentialEpoch;

#[tokio::test]
async fn every_failed_write_stage_rolls_back_password_epoch_intent_tokens_and_counter() {
    for reset in [false, true] {
        for stage in ["allocator", "password", "intent", "links", "commit"] {
            let fixture = Fixture::new().await;
            let before = fixture.state().await;
            let sql = match stage {
                "allocator" => {
                    "CREATE TRIGGER fixture_fail BEFORE UPDATE ON credential_epoch_allocator BEGIN SELECT RAISE(ABORT,'fixture allocator'); END"
                }
                "password" => {
                    "CREATE TRIGGER fixture_fail BEFORE UPDATE OF password_hash ON user_logins BEGIN SELECT RAISE(ABORT,'fixture password'); END"
                }
                "intent" => {
                    "CREATE TRIGGER fixture_fail BEFORE INSERT ON credential_revocations BEGIN SELECT RAISE(ABORT,'fixture intent'); END"
                }
                "links" => {
                    "CREATE TRIGGER fixture_fail BEFORE UPDATE ON email_tokens WHEN OLD.purpose='reset' AND hex(OLD.digest)=printf('%064d',0) BEGIN SELECT RAISE(ABORT,'fixture links'); END"
                }
                "commit" => {
                    "CREATE TABLE deferred_failure(account_id INTEGER REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fixture_fail AFTER INSERT ON credential_revocations BEGIN INSERT INTO deferred_failure VALUES(999999); END"
                }
                _ => unreachable!(),
            };
            if stage == "links" {
                // The second valid reset link is made all-zero solely to target the
                // common final revocation, after reset's first one-use claim.
                fixture.execute("UPDATE email_tokens SET digest=zeroblob(32) WHERE digest=(SELECT digest FROM email_tokens WHERE purpose='reset' ORDER BY digest DESC LIMIT 1)").await;
            }
            let before = if stage == "links" { fixture.state().await } else { before };
            fixture.execute(sql).await;
            let failed =
                if reset { fixture.reset().await.is_err() } else { fixture.self_change(OLD, CredentialEpoch::INITIAL).await.is_err() };
            assert!(failed, "reset={reset},stage={stage}");
            assert_eq!(fixture.state().await, before, "no partial authority after reset={reset},stage={stage}");
            fixture.execute("DROP TRIGGER fixture_fail").await;
            if reset {
                assert!(fixture.reset().await.unwrap().is_some());
            } else {
                assert!(fixture.self_change(OLD, CredentialEpoch::INITIAL).await.unwrap().is_some());
            }
        }
    }
}

#[tokio::test]
async fn exhausted_missing_and_lagging_allocator_cannot_spend_a_valid_reset_link() {
    for state in ["exhausted", "missing", "lagging"] {
        let fixture = Fixture::new().await;
        match state {
            "exhausted" => fixture.execute("UPDATE credential_epoch_allocator SET high_water=9223372036854775807").await,
            "missing" => fixture.execute("DELETE FROM credential_epoch_allocator").await,
            "lagging" => fixture.execute("UPDATE user_logins SET authentication_epoch=8").await,
            _ => unreachable!(),
        }
        // Missing singleton cannot use State's nonoptional counter projection.
        let before = if state == "missing" { None } else { Some(fixture.state().await) };
        assert!(fixture.reset().await.is_err(), "{state}");
        if let Some(before) = before {
            assert_eq!(fixture.state().await, before);
        } else {
            let login: (String, i64) =
                sqlx::query_as("SELECT password_hash,authentication_epoch FROM user_logins").fetch_one(&fixture.raw).await.unwrap();
            assert_eq!(login, (OLD.into(), 0));
            let used: i64 =
                sqlx::query_scalar("SELECT count(*) FROM email_tokens WHERE used_at IS NOT NULL").fetch_one(&fixture.raw).await.unwrap();
            let intents: i64 = sqlx::query_scalar("SELECT count(*) FROM credential_revocations").fetch_one(&fixture.raw).await.unwrap();
            assert_eq!((used, intents), (0, 0));
        }
    }
}

#[tokio::test]
async fn original_writer_does_not_silently_activate_legacy_calls_or_finish_its_intent() {
    let fixture = Fixture::new().await;
    let intent = fixture.self_change(OLD, CredentialEpoch::INITIAL).await.unwrap().unwrap();
    fixture.database.profiles().set_password_hash(fixture.account, "legacy compatibility").await.unwrap();
    let state = fixture.state().await;
    assert_eq!(state.login.0, "legacy compatibility");
    assert_eq!(state.login.1, intent.target_epoch);
    assert_eq!(state.intents, [(fixture.account.0, intent.target_epoch, None)]);
    // This intentionally demonstrates why mixed legacy writers are an activation
    // blocker. The dormant API is not a persistent mode/version interlock.
}
