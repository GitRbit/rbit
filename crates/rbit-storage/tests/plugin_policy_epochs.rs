//! Real bounded policy epochs, never executable authorization.
use super::*;
use rbit_plugin_api::PackagePolicyRenewalRequest;
use rbit_storage::sqlite::plugin_policy::{PolicyGate, PolicyRenewalOutcome};

fn renewal_request(gate: &PolicyGate) -> PackagePolicyRenewalRequest {
    PackagePolicyRenewalRequest::new(gate.fingerprint.clone().unwrap(), gate.seal.as_ref().unwrap().operation.as_str()).unwrap()
}
fn fresh_review(fixture: &Fixture, gate: &PolicyGate) -> PackageReviewRequest {
    assert_eq!(gate.state, PolicyGateState::Open);
    PackageReviewRequest::new(
        fixture.package.clone(),
        ExpectedPolicyRevision::Absent {},
        gate.fingerprint.clone().unwrap(),
        PackageReviewIntent::DataOnly {},
        PolicyGrantRequests::new(Vec::new()).unwrap(),
    )
    .unwrap()
}
async fn ledger_counts(fixture: &Fixture) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT COUNT(*) FROM plugin_policy_epochs),(SELECT COUNT(*) FROM plugin_policy_renewals),(SELECT COUNT(*) FROM plugin_review_history),(SELECT COUNT(*) FROM plugin_policy_receipts)")
        .fetch_one(&mut fixture.connection().await).await.unwrap()
}
async fn epoch_counts(fixture: &Fixture, epoch: &rbit_plugin_api::Sha256Digest) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT COUNT(*) FROM plugin_review_history WHERE epoch=?),(SELECT COUNT(*) FROM plugin_policy_receipts WHERE epoch=?),(SELECT COUNT(*) FROM plugin_review_heads WHERE epoch=?),(SELECT COUNT(*) FROM plugin_selection_heads WHERE epoch=?)")
        .bind(epoch.as_str()).bind(epoch.as_str()).bind(epoch.as_str()).bind(epoch.as_str())
        .fetch_one(&mut fixture.connection().await).await.unwrap()
}
async fn promote_other(fixture: &Fixture) {
    assert_eq!(
        sqlx::query("UPDATE user_logins SET is_admin=1 WHERE account_id=?")
            .bind(fixture.other.0)
            .execute(&mut fixture.connection().await)
            .await
            .unwrap()
            .rows_affected(),
        1
    );
}

// One real seed (+ optional real selection), valid bulk sequential decisions in
// ONE SQLite transaction, then a STORE mutation that commits an actual capacity seal.
// No hand-written sealed gate or pretend exhausted history enters these fixtures.
async fn seal_current(fixture: &Fixture, request: &PackageReviewRequest, seed: &str, seal: &str, selected: bool) -> PolicyGate {
    let store = fixture.db.plugin_policy();
    assert_eq!(applied(store.apply_review(fixture.admin, &id(seed), request, Timestamp(10)).await.unwrap()), rev(1));
    if selected {
        let select = PackageSelectionRequest::new(
            fixture.package.clone(),
            ExpectedPolicyRevision::Absent {},
            rev(1),
            request.expected_host_policy.clone(),
        );
        assert_eq!(
            applied(store.apply_selection(fixture.admin, &id(&format!("{seed}-select")), &select, Timestamp(11)).await.unwrap()),
            rev(1)
        );
    }
    fill_history(fixture).await;
    let current = store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.unwrap().revision;
    assert_eq!(current, rev(if selected { 4095 } else { 4096 }));
    let mut next = request.clone();
    next.expected_review = ExpectedPolicyRevision::Exact { revision: current };
    next.intent = PackageReviewIntent::Deny {};
    let outcome = store.apply_review(fixture.admin, &id(seal), &next, Timestamp(12)).await.unwrap();
    let ApplyOutcome::CapacitySealed { gate } = outcome else {
        panic!("expected real capacity seal: {outcome:?}");
    };
    assert_eq!(gate.seal.as_ref().unwrap().operation, id(seal));
    assert_eq!(
        gate.seal.as_ref().unwrap().reason,
        if selected { PolicySealReason::ReceiptCapacity } else { PolicySealReason::HistoryCapacity }
    );
    assert!(store.operation(fixture.admin, &id(seal)).await.unwrap().is_none());
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    gate
}

async fn assert_first_recovery(
    fixture: &Fixture,
    original: &PackageReviewRequest,
    old: &PolicyGate,
    renewed: &PolicyGate,
    request: &PackagePolicyRenewalRequest,
) {
    let store = fixture.db.plugin_policy();
    assert_ne!(renewed.epoch, old.epoch);
    assert_ne!(renewed.fingerprint, old.fingerprint);
    let snapshot = store.inspect(fixture.admin, fixture.package.plugin_id()).await.unwrap();
    assert!(snapshot.review.is_none());
    assert!(snapshot.selection.is_none());
    assert_eq!(epoch_counts(fixture, old.epoch.as_ref().unwrap()).await, (4095, 4096, 1, 1));
    assert_eq!(epoch_counts(fixture, renewed.epoch.as_ref().unwrap()).await, (0, 0, 0, 0));
    assert!(matches!(
        store.apply_review(fixture.admin, &id("epoch0-seed"), original, Timestamp(20)).await.unwrap(),
        ApplyOutcome::HistoricalReceipt { .. }
    ));
    conflict(store.apply_review(fixture.admin, &id("old-unrecorded"), original, Timestamp(20)).await);
    let fresh = fresh_review(fixture, renewed);
    for key in ["epoch0-seal", "renew-0"] {
        conflict(store.apply_review(fixture.admin, &id(key), &fresh, Timestamp(20)).await);
    }
    assert!(store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.is_none());
    assert!(matches!(
        store.renew(fixture.admin, &id("renew-0"), request, Timestamp(20)).await.unwrap(),
        PolicyRenewalOutcome::HistoricalReceipt { .. }
    ));
    let mut different = request.clone();
    "different-seal".clone_into(&mut different.expected_seal_operation);
    conflict(store.renew(fixture.admin, &id("renew-0"), &different, Timestamp(20)).await);
    promote_other(fixture).await;
    conflict(store.renew(fixture.other, &id("renew-0"), request, Timestamp(20)).await);
    assert_eq!(store.gate(fixture.admin).await.unwrap(), *renewed);
    assert_eq!(fixture.db.plugin_settings().desired_enabled(fixture.package.plugin_id().as_str()).await.unwrap(), None);
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
}

#[tokio::test]
async fn real_capacity_renewal_retains_audit_resets_comparisons_and_stops_at_two_retired_epochs() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let original = fixture.review().await;
    let sealed0 = seal_current(&fixture, &original, "epoch0-seed", "epoch0-seal", true).await;
    let request0 = renewal_request(&sealed0);
    conflict(store.renew(fixture.admin, &id("epoch0-seed"), &request0, Timestamp(13)).await);
    conflict(store.renew(fixture.admin, &id("epoch0-seal"), &request0, Timestamp(13)).await);
    assert_eq!(store.gate(fixture.admin).await.unwrap(), sealed0);
    let outcome = store.renew(fixture.admin, &id("renew-0"), &request0, Timestamp(13)).await.unwrap();
    let PolicyRenewalOutcome::Applied { receipt: receipt0, gate: gate1 } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(receipt0.source_epoch, sealed0.epoch.clone().unwrap());
    assert_eq!(receipt0.destination_epoch, gate1.epoch.clone().unwrap());
    assert_first_recovery(&fixture, &original, &sealed0, &gate1, &request0).await;
    let review1 = fresh_review(&fixture, &gate1);
    let sealed1 = seal_current(&fixture, &review1, "epoch1-seed", "epoch1-seal", false).await;
    let request1 = renewal_request(&sealed1);
    let outcome = store.renew(fixture.admin, &id("renew-1"), &request1, Timestamp(30)).await.unwrap();
    let PolicyRenewalOutcome::Applied { gate: gate2, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(ledger_counts(&fixture).await, (3, 2, 8191, 8192));
    // First receipt remains recoverable even though its destination is now retired.
    assert_eq!(store.renewal_operation(fixture.admin, &id("renew-0")).await.unwrap(), Some(receipt0.clone()));
    assert!(matches!(
        store.renew(fixture.admin, &id("renew-0"), &request0, Timestamp(31)).await.unwrap(),
        PolicyRenewalOutcome::HistoricalReceipt { .. }
    ));
    assert_eq!(store.gate(fixture.admin).await.unwrap(), gate2);
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    let review2 = fresh_review(&fixture, &gate2);
    let sealed2 = seal_current(&fixture, &review2, "epoch2-seed", "epoch2-seal", false).await;
    let before = ledger_counts(&fixture).await;
    let outcome = store.renew(fixture.admin, &id("renew-retention-full"), &renewal_request(&sealed2), Timestamp(40)).await.unwrap();
    let PolicyRenewalOutcome::RetentionFull { gate } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(gate, sealed2);
    assert_eq!(before, (3, 2, 12287, 12288));
    assert_eq!(ledger_counts(&fixture).await, before);
    assert!(store.renewal_operation(fixture.admin, &id("renew-retention-full")).await.unwrap().is_none());
    assert_eq!(store.gate(fixture.admin).await.unwrap().state, PolicyGateState::Sealed);
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    fixture.db.close().await;
}

#[tokio::test]
async fn concurrent_renewal_cas_has_one_winner_and_survives_restart() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    let sealed = seal_current(&fixture, &request, "race-seed", "race-seal", false).await;
    let renewal = renewal_request(&sealed);
    let first = id("race-renew-one");
    let second = id("race-renew-two");
    let (first, second) = tokio::join!(
        store.renew(fixture.admin, &first, &renewal, Timestamp(20)),
        store.renew(fixture.admin, &second, &renewal, Timestamp(20))
    );
    let results = [first, second];
    assert_eq!(results.iter().filter(|r| matches!(r, Ok(PolicyRenewalOutcome::Applied { .. }))).count(), 1);
    assert_eq!(results.iter().filter(|r| matches!(r, Err(StorageError::Conflict(_)))).count(), 1);
    let (receipt, gate) = results
        .into_iter()
        .find_map(|r| match r {
            Ok(PolicyRenewalOutcome::Applied { receipt, gate }) => Some((receipt, gate)),
            _ => None,
        })
        .unwrap();
    assert_eq!(ledger_counts(&fixture).await, (2, 1, 4096, 4096));
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    fixture.db.close().await;
    let reopened = Database::open(&fixture.path).await.unwrap();
    assert_eq!(reopened.plugin_policy().gate(fixture.admin).await.unwrap(), gate);
    assert_eq!(reopened.plugin_policy().renewal_operation(fixture.admin, &receipt.id).await.unwrap(), Some(receipt));
    assert!(reopened.plugin_policy().inspect_package(fixture.admin, &fixture.package).await.unwrap().review.is_none());
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    reopened.close().await;
}

#[tokio::test]
async fn sql_abort_before_renewal_receipt_rolls_back_retirement_and_can_retry_same_key() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    let sealed = seal_current(&fixture, &request, "fault-seed", "fault-seal", false).await;
    let renewal = renewal_request(&sealed);
    let operation = id("fault-renewal");
    let before = ledger_counts(&fixture).await;
    let mut connection = fixture.connection().await;
    sqlx::query("CREATE TRIGGER test_reject_renewal BEFORE INSERT ON plugin_policy_renewals BEGIN SELECT RAISE(ABORT,'injected renewal receipt failure'); END").execute(&mut connection).await.unwrap();
    // This is a real SQL ABORT after staged retirement/new-row writes, NOT an
    // uncertain fsync/COMMIT simulation or proof of publication after waiter loss.
    assert!(matches!(store.renew(fixture.admin, &operation, &renewal, Timestamp(20)).await, Err(StorageError::Sqlite(_))));
    assert_eq!(store.gate(fixture.admin).await.unwrap(), sealed);
    assert_eq!(ledger_counts(&fixture).await, before);
    assert!(store.renewal_operation(fixture.admin, &operation).await.unwrap().is_none());
    validate_snapshot(&mut connection).await.unwrap();
    sqlx::query("DROP TRIGGER test_reject_renewal").execute(&mut connection).await.unwrap();
    assert!(matches!(store.renew(fixture.admin, &operation, &renewal, Timestamp(21)).await.unwrap(), PolicyRenewalOutcome::Applied { .. }));
    assert_eq!(ledger_counts(&fixture).await, (2, 1, 4096, 4096));
    validate_snapshot(&mut connection).await.unwrap();
    connection.close().await.unwrap();
    fixture.db.close().await;
}

#[tokio::test]
async fn renewal_actor_fk_scrubbing_prevents_claim_by_reused_numeric_account_id() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    let sealed = seal_current(&fixture, &request, "actor-seed", "actor-seal", false).await;
    let renewal = renewal_request(&sealed);
    let operation = id("actor-renewal");
    store.renew(fixture.admin, &operation, &renewal, Timestamp(20)).await.unwrap();
    promote_other(&fixture).await;
    let mut connection = fixture.connection().await;
    sqlx::query("DELETE FROM accounts WHERE id=?").bind(fixture.admin.0).execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(?,'user','epoch-replacement',1,1)")
        .bind(fixture.admin.0)
        .execute(&mut connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_logins(account_id,email,password_hash,is_admin) VALUES(?,'epoch-replacement@example.com','unused',1)")
        .bind(fixture.admin.0)
        .execute(&mut connection)
        .await
        .unwrap();
    let receipt = store.renewal_operation(fixture.other, &operation).await.unwrap().unwrap();
    assert_eq!(receipt.actor, None);
    assert_eq!(receipt.request, renewal);
    conflict(store.renew(fixture.admin, &operation, &renewal, Timestamp(21)).await);
    let actor_references:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM plugin_policy_gate WHERE prepared_by IS NOT NULL OR seal_actor IS NOT NULL)+(SELECT COUNT(*) FROM plugin_policy_epochs WHERE prepared_by IS NOT NULL OR retired_by IS NOT NULL OR seal_actor IS NOT NULL)+(SELECT COUNT(*) FROM plugin_policy_renewals WHERE actor IS NOT NULL)+(SELECT COUNT(*) FROM plugin_review_history WHERE actor IS NOT NULL)+(SELECT COUNT(*) FROM plugin_selection_heads WHERE actor IS NOT NULL)+(SELECT COUNT(*) FROM plugin_policy_receipts WHERE actor IS NOT NULL)").fetch_one(&mut connection).await.unwrap();
    assert_eq!(actor_references, 0);
    assert_eq!(store.gate(fixture.other).await.unwrap().state, PolicyGateState::Open);
    validate_snapshot(&mut connection).await.unwrap();
    connection.close().await.unwrap();
    fixture.db.close().await;
}
