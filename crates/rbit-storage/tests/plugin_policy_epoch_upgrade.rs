//! Real `SQLx` migration from 0064 to 0065 of populated v1 metadata.
//! Installation is a trusted fixture assertion, not archive verification.
use super::*;
use rbit_plugin_api::{HostPolicyFingerprint, PackagePolicyRenewalRequest};
use rbit_storage::sqlite::plugin_policy::{PolicyCommand, PolicyGate, PolicyRenewalOutcome};

const V1_EPOCH: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const V1_FINGERPRINT: &str = "dac397089dbd3401ee998fde15507510ac5dd3fddf60c2e1ee89e992646d08d2";

struct LegacySeed {
    review: PackageReviewRequest,
    selection: PackageSelectionRequest,
    review_wire: String,
    selection_wire: String,
    seal_wire: String,
}
fn legacy_seed(fixture: &Fixture) -> LegacySeed {
    let fingerprint = HostPolicyFingerprint::new(V1_FINGERPRINT.parse().unwrap());
    let review = PackageReviewRequest::new(
        fixture.package.clone(),
        ExpectedPolicyRevision::Absent {},
        fingerprint.clone(),
        PackageReviewIntent::DataOnly {},
        PolicyGrantRequests::new(Vec::new()).unwrap(),
    )
    .unwrap();
    let selection = PackageSelectionRequest::new(fixture.package.clone(), ExpectedPolicyRevision::Absent {}, rev(1), fingerprint);
    let mut next = review.clone();
    next.expected_review = ExpectedPolicyRevision::Exact { revision: rev(1) };
    next.intent = PackageReviewIntent::Deny {};
    LegacySeed {
        review_wire: PolicyCommand::Review(review.clone()).canonical_request().unwrap(),
        selection_wire: PolicyCommand::Select(selection.clone()).canonical_request().unwrap(),
        seal_wire: PolicyCommand::Review(next).canonical_request().unwrap(),
        review,
        selection,
    }
}

async fn recreate_legacy_graph(fixture: &Fixture, seed: &LegacySeed, sealed: bool) {
    // The CURRENT database is closed before test-only schema replacement. No
    // current store/API is invoked while the legacy five-table graph is present.
    fixture.db.close().await;
    let mut connection = fixture.connection().await;
    let foreign_keys: bool = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&mut connection).await.unwrap();
    assert!(foreign_keys);
    let virgin:bool=sqlx::query_scalar("SELECT (SELECT state FROM plugin_policy_gate)=0 AND NOT EXISTS(SELECT 1 FROM plugin_policy_epochs) AND NOT EXISTS(SELECT 1 FROM plugin_policy_renewals) AND NOT EXISTS(SELECT 1 FROM plugin_review_history) AND NOT EXISTS(SELECT 1 FROM plugin_review_heads) AND NOT EXISTS(SELECT 1 FROM plugin_selection_heads) AND NOT EXISTS(SELECT 1 FROM plugin_policy_receipts)").fetch_one(&mut connection).await.unwrap();
    assert!(virgin, "test downgrade must never discard an existing current policy");
    let mut tx = connection.begin().await.unwrap();
    sqlx::raw_sql("DROP TABLE plugin_policy_receipts; DROP TABLE plugin_selection_heads; DROP TABLE plugin_review_heads; DROP TABLE plugin_review_history; DROP TABLE plugin_policy_gate; DROP TABLE plugin_policy_renewals; DROP TABLE plugin_policy_epochs; DROP INDEX plugin_installations_exact; DELETE FROM _sqlx_migrations WHERE version>=65;").execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0064_plugin_policy.sql")).execute(&mut *tx).await.unwrap();
    seed_legacy_rows(&mut tx, fixture, seed, sealed).await;
    tx.commit().await.unwrap();
    // Raw validator dispatches to the FROZEN0064 decoder, with no65 marker.
    validate_snapshot(&mut connection).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version>=65").fetch_one(&mut connection).await.unwrap(),
        0
    );
    connection.close().await.unwrap();
}
async fn seed_legacy_rows(connection: &mut SqliteConnection, fixture: &Fixture, seed: &LegacySeed, sealed: bool) {
    sqlx::query("UPDATE plugin_policy_gate SET state=1,epoch=?,fingerprint=?,prepared_by=?,prepared_at=99 WHERE id=1")
        .bind(V1_EPOCH)
        .bind(V1_FINGERPRINT)
        .bind(fixture.admin.0)
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plugin_review_history(digest,revision,plugin_id,version,intent,request,catalog_version,epoch,fingerprint,actor,created_at) VALUES(?,1,?,?,1,?,'empty-v1',?,?,?,100)")
        .bind(fixture.package.archive_digest().as_str()).bind(fixture.package.plugin_id().as_str()).bind(fixture.package.version().to_string()).bind(&seed.review_wire)
        .bind(V1_EPOCH).bind(V1_FINGERPRINT).bind(fixture.admin.0).execute(&mut *connection).await.unwrap();
    sqlx::query("INSERT INTO plugin_review_heads(digest,revision) VALUES(?,1)")
        .bind(fixture.package.archive_digest().as_str())
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO plugin_selection_heads(plugin_id,revision,version,digest,review_revision,actor,updated_at) VALUES(?,1,?,?,1,?,101)",
    )
    .bind(fixture.package.plugin_id().as_str())
    .bind(fixture.package.version().to_string())
    .bind(fixture.package.archive_digest().as_str())
    .bind(fixture.admin.0)
    .execute(&mut *connection)
    .await
    .unwrap();
    for (id, kind, wire, time) in [("legacy-review", 0, &seed.review_wire, 100_i64), ("legacy-select", 1, &seed.selection_wire, 101)] {
        sqlx::query("INSERT INTO plugin_policy_receipts(id,kind,request,actor,plugin_id,digest,result_revision,epoch,fingerprint,created_at) VALUES(?,?,?,?,?,?,1,?,?,?)")
            .bind(id).bind(kind).bind(wire).bind(fixture.admin.0).bind(fixture.package.plugin_id().as_str()).bind(fixture.package.archive_digest().as_str())
            .bind(V1_EPOCH).bind(V1_FINGERPRINT).bind(time).execute(&mut *connection).await.unwrap();
    }
    if sealed {
        // Synthetic ALREADY-SEALED legacy state, intentionally not evidence that
        // capacity was reached; separate real4096 tests prove actual sealing.
        sqlx::query("UPDATE plugin_policy_gate SET state=2,seal_reason=0,seal_actor=?,seal_operation='legacy-existing-seal',seal_request=?,sealed_at=102")
            .bind(fixture.admin.0).bind(&seed.seal_wire).execute(connection).await.unwrap();
    }
}

async fn assert_migrated_rows(fixture: &Fixture, database: &Database, seed: &LegacySeed, sealed: bool) -> PolicyGate {
    let store = database.plugin_policy();
    let gate = store.gate(fixture.admin).await.unwrap();
    assert_eq!(gate.state, if sealed { PolicyGateState::Sealed } else { PolicyGateState::Open });
    assert_eq!(gate.epoch.as_ref().unwrap().as_str(), V1_EPOCH);
    assert_eq!(gate.fingerprint.as_ref().unwrap().as_str(), V1_FINGERPRINT);
    if sealed {
        let seal = gate.seal.as_ref().unwrap();
        assert_eq!(seal.operation, id("legacy-existing-seal"));
        assert_eq!(seal.actor, Some(fixture.admin));
        assert_eq!(seal.at, Timestamp(102));
        assert_eq!(seal.request, seed.seal_wire);
    } else {
        assert!(gate.seal.is_none());
    }
    let selected = store.inspect(fixture.admin, fixture.package.plugin_id()).await.unwrap();
    let head = selected.selection.unwrap();
    assert_eq!(head.package, Some(fixture.package.clone()));
    assert_eq!(head.revision, rev(1));
    assert_eq!(head.review_revision, Some(rev(1)));
    assert_eq!(head.actor, Some(fixture.admin));
    assert_eq!(head.updated_at, Timestamp(101));
    let reviewed = store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.unwrap();
    assert_eq!(reviewed.request, seed.review);
    assert_eq!(reviewed.revision, rev(1));
    assert_eq!(reviewed.actor, Some(fixture.admin));
    assert_eq!(reviewed.created_at, Timestamp(100));
    for (key, command, time) in [
        ("legacy-review", PolicyCommand::Review(seed.review.clone()), 100_i64),
        ("legacy-select", PolicyCommand::Select(seed.selection.clone()), 101),
    ] {
        let receipt = store.operation(fixture.admin, &id(key)).await.unwrap().unwrap();
        assert_eq!(receipt.command, command);
        assert_eq!(receipt.result_revision, rev(1));
        assert_eq!(receipt.actor, Some(fixture.admin));
        assert_eq!(receipt.created_at, Timestamp(time));
        assert_eq!(receipt.epoch.as_str(), V1_EPOCH);
        assert_eq!(receipt.fingerprint.as_str(), V1_FINGERPRINT);
    }
    assert_original_bytes(fixture, seed).await;
    let mut connection = fixture.connection().await;
    let registry: (i64, i64, i64, Option<i64>, i64) =
        sqlx::query_as("SELECT ordinal,descriptor_version,retired,prepared_by,prepared_at FROM plugin_policy_epochs WHERE epoch=?")
            .bind(V1_EPOCH)
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(registry, (0, 1, 0, Some(fixture.admin.0), 99));
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT success FROM _sqlx_migrations WHERE version=65").fetch_one(&mut connection).await.unwrap()
    );
    validate_snapshot(&mut connection).await.unwrap();
    conflict(store.prepare(fixture.admin, Timestamp(103)).await);
    connection.close().await.unwrap();
    gate
}
async fn assert_original_bytes(fixture: &Fixture, seed: &LegacySeed) {
    let mut connection = fixture.connection().await;
    let history: Vec<u8> =
        sqlx::query_scalar("SELECT CAST(request AS BLOB) FROM plugin_review_history WHERE epoch=? AND digest=? AND revision=1")
            .bind(V1_EPOCH)
            .bind(fixture.package.archive_digest().as_str())
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(history, seed.review_wire.as_bytes().to_vec());
    for (key, wire) in [("legacy-review", &seed.review_wire), ("legacy-select", &seed.selection_wire)] {
        let (actual, version): (Vec<u8>, String) =
            sqlx::query_as("SELECT CAST(request AS BLOB),version FROM plugin_policy_receipts WHERE id=?")
                .bind(key)
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(actual, wire.as_bytes().to_vec());
        assert_eq!(version, fixture.package.version().to_string());
    }
    connection.close().await.unwrap();
}
async fn renew_migrated_sealed(fixture: &Fixture, database: &Database, seed: &LegacySeed, old: &PolicyGate) {
    let store = database.plugin_policy();
    let request = PackagePolicyRenewalRequest::new(old.fingerprint.clone().unwrap(), "legacy-existing-seal").unwrap();
    let outcome = store.renew(fixture.admin, &id("legacy-epoch-renew"), &request, Timestamp(103)).await.unwrap();
    let PolicyRenewalOutcome::Applied { receipt, gate } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(receipt.source_epoch.as_str(), V1_EPOCH);
    assert_ne!(gate.epoch, old.epoch);
    assert_ne!(gate.fingerprint, old.fingerprint);
    assert_eq!(gate.state, PolicyGateState::Open);
    let fresh = store.inspect(fixture.admin, fixture.package.plugin_id()).await.unwrap();
    assert!(fresh.review.is_none());
    assert!(fresh.selection.is_none());
    // Old known receipt acknowledges history; it cannot restore the retired head.
    assert!(matches!(
        store.apply_review(fixture.admin, &id("legacy-review"), &seed.review, Timestamp(104)).await.unwrap(),
        ApplyOutcome::HistoricalReceipt { .. }
    ));
    conflict(store.apply_review(fixture.admin, &id("legacy-unrecorded"), &seed.review, Timestamp(104)).await);
    assert!(store.inspect(fixture.admin, fixture.package.plugin_id()).await.unwrap().selection.is_none());
    assert!(store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.is_none());
    assert_original_bytes(fixture, seed).await;
    let mut connection = fixture.connection().await;
    let versions: Vec<(i64, i64, i64)> =
        sqlx::query_as("SELECT ordinal,descriptor_version,retired FROM plugin_policy_epochs ORDER BY ordinal")
            .fetch_all(&mut connection)
            .await
            .unwrap();
    assert_eq!(versions, vec![(0, 1, 1), (1, 2, 0)]);
    validate_snapshot(&mut connection).await.unwrap();
    connection.close().await.unwrap();
}

#[tokio::test]
async fn actual_database_open_upgrades_populated_v1_open_and_sealed_graphs_without_reencoding_audit() {
    for sealed in [false, true] {
        let fixture = Fixture::new().await;
        let seed = legacy_seed(&fixture);
        recreate_legacy_graph(&fixture, &seed, sealed).await;
        // This invokes the actual compiled SQLx migrator through public open.
        let upgraded = Database::open(&fixture.path).await.unwrap();
        let gate = assert_migrated_rows(&fixture, &upgraded, &seed, sealed).await;
        if sealed {
            renew_migrated_sealed(&fixture, &upgraded, &seed, &gate).await;
        } else {
            assert!(matches!(
                upgraded.plugin_policy().apply_review(fixture.admin, &id("legacy-review"), &seed.review, Timestamp(103)).await.unwrap(),
                ApplyOutcome::HistoricalReceipt { .. }
            ));
            assert_eq!(upgraded.plugin_policy().gate(fixture.admin).await.unwrap(), gate);
        }
        assert_eq!(upgraded.plugin_settings().desired_enabled(fixture.package.plugin_id().as_str()).await.unwrap(), None);
        upgraded.close().await;
    }
}
