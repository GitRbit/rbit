//! Passive policy transactions, permanent capacity seal and identity/replay boundaries.
use rbit_core::{AccountId, Timestamp};
use rbit_plugin_api::{
    ExpectedPolicyRevision, InstalledPackageRef, PackageManifest, PackageReviewIntent, PackageReviewRequest, PackageSelectionClearRequest,
    PackageSelectionRequest, PolicyGrantRequests, PolicyRevision,
};
use rbit_storage::{
    Database, NewUser, StorageError,
    sqlite::{
        plugin_installations::{InstallationDescriptor, OperationId},
        plugin_policy::{ApplyOutcome, PolicyGateState, PolicySealReason, validate_snapshot},
    },
};
use serde_json::json;
use sqlx::{
    Connection as _,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};

struct Fixture {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    db: Database,
    admin: AccountId,
    other: AccountId,
    package: InstalledPackageRef,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rbit.db");
        let db = Database::open(&path).await.unwrap();
        let admin = db
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "unused".into(),
                is_admin: true,
            })
            .await
            .unwrap()
            .id();
        let other = db
            .accounts()
            .create_user(NewUser {
                name: "bob".parse().unwrap(),
                email: "bob@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "unused".into(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id();
        let manifest:PackageManifest=serde_json::from_value(json!({"schema_version":1,"plugin":{"id":"example.greeter","version":"1.0.0+first","host_api":"^1"},"kind":{"type":"data"},"files":[{"path":"message.txt","size":1,"sha256":"a".repeat(64)}],"budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}})).unwrap();
        let digest: rbit_plugin_api::Sha256Digest = "a".repeat(64).parse().unwrap();
        let package = InstalledPackageRef::new(manifest.plugin.id.clone(), manifest.plugin.version.clone(), digest.clone()).unwrap();
        let descriptor = InstallationDescriptor::new(manifest, digest, 4096).unwrap();
        let install = id("installation");
        db.plugin_installations().begin(&install, &descriptor, Some(admin), Timestamp(1)).await.unwrap();
        db.plugin_installations().complete(&install, 0, Timestamp(2)).await.unwrap();
        Self { _dir: dir, path, db, admin, other, package }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&self.path).foreign_keys(true)).await.unwrap()
    }
    async fn review(&self) -> PackageReviewRequest {
        let gate = self.db.plugin_policy().prepare(self.admin, Timestamp(3)).await.unwrap();
        PackageReviewRequest::new(
            self.package.clone(),
            ExpectedPolicyRevision::Absent {},
            gate.fingerprint.unwrap(),
            PackageReviewIntent::DataOnly {},
            PolicyGrantRequests::new(Vec::new()).unwrap(),
        )
        .unwrap()
    }
}
fn id(value: &str) -> OperationId {
    OperationId::new(value).unwrap()
}
fn rev(value: u64) -> PolicyRevision {
    PolicyRevision::new(value).unwrap()
}
fn applied(outcome: ApplyOutcome) -> PolicyRevision {
    let ApplyOutcome::Applied { receipt, .. } = outcome else {
        panic!("expected applied: {outcome:?}");
    };
    receipt.result_revision
}
fn conflict<T: std::fmt::Debug>(result: Result<T, StorageError>) {
    let error = result.unwrap_err();
    assert!(matches!(error, StorageError::Conflict(_)), "{error:?}");
}

#[tokio::test]
async fn explicit_bootstrap_and_exact_review_selection_survive_restart_without_enabled_intent() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    assert_eq!(store.gate(fixture.admin).await.unwrap().state, PolicyGateState::Uninitialized);
    let empty = store.inspect_package(fixture.admin, &fixture.package).await.unwrap();
    assert_eq!(empty.gate.state, PolicyGateState::Uninitialized);
    assert!(matches!(empty.declarations.kind, rbit_plugin_api::PackageKind::Data {}));
    assert_eq!(empty.declarations.capabilities, Vec::<rbit_plugin_api::CapabilityId>::new());
    assert_eq!(empty.declarations.host_api.to_string(), "^1");
    assert_eq!(empty.declarations.budgets.memory_bytes, 1024);
    assert!(empty.review.is_none());
    conflict(store.prepare(fixture.other, Timestamp(3)).await);
    let request = fixture.review().await;
    let original = store.gate(fixture.admin).await.unwrap();
    conflict(store.prepare(fixture.admin, Timestamp(4)).await);
    assert_eq!(applied(store.apply_review(fixture.admin, &id("review"), &request, Timestamp(4)).await.unwrap()), rev(1));
    let select = PackageSelectionRequest::new(
        fixture.package.clone(),
        ExpectedPolicyRevision::Absent {},
        rev(1),
        request.expected_host_policy.clone(),
    );
    assert_eq!(applied(store.apply_selection(fixture.admin, &id("selection"), &select, Timestamp(5)).await.unwrap()), rev(1));
    let snapshot = store.inspect(fixture.admin, fixture.package.plugin_id()).await.unwrap();
    assert_eq!(snapshot.gate, original);
    assert_eq!(snapshot.selection.unwrap().package, Some(fixture.package.clone()));
    assert_eq!(fixture.db.plugin_settings().desired_enabled("example.greeter").await.unwrap(), None);
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    fixture.db.close().await;
    let reopened = Database::open(&fixture.path).await.unwrap();
    assert_eq!(reopened.plugin_policy().gate(fixture.admin).await.unwrap(), original);
    assert_eq!(reopened.plugin_policy().inspect_package(fixture.admin, &fixture.package).await.unwrap().review.unwrap().revision, rev(1));
    reopened.close().await;
}

#[tokio::test]
async fn replay_is_historical_and_tombstones_cannot_reset_selection_cas() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    store.apply_review(fixture.admin, &id("review"), &request, Timestamp(4)).await.unwrap();
    let select = PackageSelectionRequest::new(
        fixture.package.clone(),
        ExpectedPolicyRevision::Absent {},
        rev(1),
        request.expected_host_policy.clone(),
    );
    store.apply_selection(fixture.admin, &id("select"), &select, Timestamp(5)).await.unwrap();
    let clear = PackageSelectionClearRequest::new(
        fixture.package.plugin_id().clone(),
        ExpectedPolicyRevision::Exact { revision: rev(1) },
        request.expected_host_policy.clone(),
    );
    assert_eq!(applied(store.apply_clear(fixture.admin, &id("clear"), &clear, Timestamp(6)).await.unwrap()), rev(2));
    assert!(matches!(
        store.apply_selection(fixture.admin, &id("select"), &select, Timestamp(7)).await.unwrap(),
        ApplyOutcome::HistoricalReceipt { .. }
    ));
    assert!(store.inspect(fixture.admin, fixture.package.plugin_id()).await.unwrap().selection.unwrap().package.is_none());
    conflict(store.apply_selection(fixture.admin, &id("new-absent"), &select, Timestamp(7)).await);
    let mut select = select;
    select.expected_selection = ExpectedPolicyRevision::Exact { revision: rev(2) };
    assert_eq!(applied(store.apply_selection(fixture.admin, &id("new-select"), &select, Timestamp(8)).await.unwrap()), rev(3));
    let mut deny = request.clone();
    deny.expected_review = ExpectedPolicyRevision::Exact { revision: rev(1) };
    deny.intent = PackageReviewIntent::Deny {};
    store.apply_review(fixture.admin, &id("deny"), &deny, Timestamp(9)).await.unwrap();
    assert!(matches!(
        store.apply_review(fixture.admin, &id("review"), &request, Timestamp(10)).await.unwrap(),
        ApplyOutcome::HistoricalReceipt { .. }
    ));
    select.expected_selection = ExpectedPolicyRevision::Exact { revision: rev(3) };
    conflict(store.apply_selection(fixture.admin, &id("revoked-select"), &select, Timestamp(10)).await);
    assert!(matches!(
        store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.unwrap().request.intent,
        PackageReviewIntent::Deny {}
    ));
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    fixture.db.close().await;
}

#[tokio::test]
async fn active_admin_and_semantic_conflicts_do_not_seal_or_change_policy() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    conflict(store.apply_review(fixture.other, &id("nonadmin"), &request, Timestamp(4)).await);
    let mut wrong_kind = request.clone();
    wrong_kind.intent = PackageReviewIntent::NativeFullServiceUserTrust {};
    let error = store.apply_review(fixture.admin, &id("kind-mismatch"), &wrong_kind, Timestamp(4)).await.unwrap_err();
    assert!(matches!(error, StorageError::Conflict("package review declaration mismatch")));
    let mut bad = request.clone();
    bad.expected_review = ExpectedPolicyRevision::Exact { revision: rev(99) };
    conflict(store.apply_review(fixture.admin, &id("stale"), &bad, Timestamp(4)).await);
    bad = request.clone();
    bad.expected_host_policy = rbit_plugin_api::HostPolicyFingerprint::new("f".repeat(64).parse().unwrap());
    conflict(store.apply_review(fixture.admin, &id("fingerprint"), &bad, Timestamp(4)).await);
    let mut connection = fixture.connection().await;
    sqlx::query("UPDATE user_logins SET password_change_required=1 WHERE account_id=?")
        .bind(fixture.admin.0)
        .execute(&mut connection)
        .await
        .unwrap();
    conflict(store.apply_review(fixture.admin, &id("must-reset"), &request, Timestamp(4)).await);
    sqlx::query("UPDATE user_logins SET password_change_required=0,suspended_at=1 WHERE account_id=?")
        .bind(fixture.admin.0)
        .execute(&mut connection)
        .await
        .unwrap();
    conflict(store.apply_review(fixture.admin, &id("suspended"), &request, Timestamp(4)).await);
    sqlx::query("UPDATE user_logins SET suspended_at=NULL,is_admin=0 WHERE account_id=?")
        .bind(fixture.admin.0)
        .execute(&mut connection)
        .await
        .unwrap();
    conflict(store.apply_review(fixture.admin, &id("demoted"), &request, Timestamp(4)).await);
    sqlx::query("UPDATE user_logins SET is_admin=1 WHERE account_id=?").bind(fixture.admin.0).execute(&mut connection).await.unwrap();
    assert_eq!(store.gate(fixture.admin).await.unwrap().state, PolicyGateState::Open);
    assert!(store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.is_none());
    validate_snapshot(&mut connection).await.unwrap();
    fixture.db.close().await;
}

// Bulk fixture is one real SQLite transaction, not thousands of FULL fsyncs.
// It builds valid canonical sequential decisions/receipts identical to store output.
async fn fill_history(fixture: &Fixture) {
    let epoch = fixture.db.plugin_policy().gate(fixture.admin).await.unwrap().epoch.unwrap();
    let mut connection = fixture.connection().await;
    let mut tx = connection.begin().await.unwrap();
    let ordinal: i64 = sqlx::query_scalar("SELECT ordinal FROM plugin_policy_epochs WHERE epoch=?")
        .bind(epoch.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let initial: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_receipts WHERE epoch=?")
        .bind(epoch.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!((1..=2).contains(&initial));
    let seed_reviews: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_review_history WHERE epoch=?")
        .bind(epoch.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(seed_reviews, 1);
    let limit = 4096 - (initial - 1);
    sqlx::query("WITH RECURSIVE n(v) AS (VALUES(2) UNION ALL SELECT v+1 FROM n WHERE v<?) INSERT INTO plugin_review_history(digest,revision,plugin_id,version,intent,request,catalog_version,epoch,fingerprint,actor,created_at) SELECT digest,v,plugin_id,version,intent,json_set(request,'$.command.request.expected_review',json_object('kind','exact','revision',CAST(v-1 AS TEXT))),catalog_version,epoch,fingerprint,actor,created_at FROM plugin_review_history,n WHERE epoch=? AND revision=1")
        .bind(limit).bind(epoch.as_str()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO plugin_policy_receipts(id,kind,request,actor,plugin_id,version,digest,result_revision,epoch,fingerprint,created_at) SELECT 'bulk-'||?||'-'||revision,0,request,actor,plugin_id,version,digest,revision,epoch,fingerprint,created_at FROM plugin_review_history WHERE epoch=? AND revision>1")
        .bind(ordinal).bind(epoch.as_str()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE plugin_review_heads SET revision=? WHERE epoch=?")
        .bind(limit)
        .bind(epoch.as_str())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn history_capacity_commits_only_permanent_seal_even_for_deny_and_replay() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    store.apply_review(fixture.admin, &id("seed"), &request, Timestamp(4)).await.unwrap();
    fill_history(&fixture).await;
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    conflict(store.apply_review(fixture.admin, &id("stale-full"), &request, Timestamp(5)).await);
    conflict(store.apply_review(fixture.other, &id("nonadmin-full"), &request, Timestamp(5)).await);
    assert_eq!(store.gate(fixture.admin).await.unwrap().state, PolicyGateState::Open);
    let mut next = request.clone();
    next.expected_review = ExpectedPolicyRevision::Exact { revision: rev(4096) };
    next.intent = PackageReviewIntent::Deny {};
    let result = store.apply_review(fixture.admin, &id("full-deny"), &next, Timestamp(5)).await.unwrap();
    let ApplyOutcome::CapacitySealed { gate } = result else {
        panic!("{result:?}");
    };
    assert_eq!(gate.seal.as_ref().unwrap().reason, PolicySealReason::HistoryCapacity);
    assert_eq!(gate.seal.as_ref().unwrap().operation, id("full-deny"));
    assert!(store.operation(fixture.admin, &id("full-deny")).await.unwrap().is_none());
    assert_eq!(store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.unwrap().revision, rev(4096));
    assert!(matches!(
        store.apply_review(fixture.admin, &id("seed"), &request, Timestamp(6)).await.unwrap(),
        ApplyOutcome::GateSealed { .. }
    ));
    conflict(store.prepare(fixture.admin, Timestamp(6)).await);
    validate_snapshot(&mut fixture.connection().await).await.unwrap();
    fixture.db.close().await;
    let reopened = Database::open(&fixture.path).await.unwrap();
    assert_eq!(reopened.plugin_policy().gate(fixture.admin).await.unwrap(), gate);
    reopened.close().await;
}

#[tokio::test]
async fn receipt_capacity_and_concurrent_last_slot_preserve_atomic_rejection() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    store.apply_review(fixture.admin, &id("seed"), &request, Timestamp(4)).await.unwrap();
    let select = PackageSelectionRequest::new(
        fixture.package.clone(),
        ExpectedPolicyRevision::Absent {},
        rev(1),
        request.expected_host_policy.clone(),
    );
    store.apply_selection(fixture.admin, &id("seed-select"), &select, Timestamp(5)).await.unwrap();
    let mut connection = fixture.connection().await;
    let mut tx = connection.begin().await.unwrap();
    sqlx::query("WITH RECURSIVE n(v) AS (VALUES(2) UNION ALL SELECT v+1 FROM n WHERE v<4094) INSERT INTO plugin_policy_receipts(id,kind,request,actor,plugin_id,version,digest,result_revision,epoch,fingerprint,created_at) SELECT 'select-'||v,1,json_set(request,'$.command.request.expected_selection',json_object('kind','exact','revision',CAST(v-1 AS TEXT))),actor,plugin_id,version,digest,v,epoch,fingerprint,created_at FROM plugin_policy_receipts,n WHERE id='seed-select'").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE plugin_selection_heads SET revision=4094").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    validate_snapshot(&mut connection).await.unwrap();
    let mut next = request;
    next.expected_review = ExpectedPolicyRevision::Exact { revision: rev(1) };
    // A review and selection clear can each consume the last receipt but do not
    // compete on the same head. The serialized loser must seal, not partially apply.
    let clear = PackageSelectionClearRequest::new(
        fixture.package.plugin_id().clone(),
        ExpectedPolicyRevision::Exact { revision: rev(4094) },
        next.expected_host_policy.clone(),
    );
    let first_id = id("last-review");
    let second_id = id("last-select");
    let (first, second) = tokio::join!(
        store.apply_review(fixture.admin, &first_id, &next, Timestamp(6)),
        store.apply_clear(fixture.admin, &second_id, &clear, Timestamp(6))
    );
    let results = [first.unwrap(), second.unwrap()];
    assert_eq!(results.iter().filter(|r| matches!(r, ApplyOutcome::Applied { .. })).count(), 1);
    assert_eq!(results.iter().filter(|r| matches!(r, ApplyOutcome::CapacitySealed { .. })).count(), 1);
    assert_eq!(store.gate(fixture.admin).await.unwrap().seal.unwrap().reason, PolicySealReason::ReceiptCapacity);
    validate_snapshot(&mut connection).await.unwrap();
    fixture.db.close().await;
}

#[tokio::test]
async fn deleted_actor_becomes_unclaimable_and_fk_scrubbing_preserves_history() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    store.apply_review(fixture.admin, &id("review"), &request, Timestamp(4)).await.unwrap();
    let select = PackageSelectionRequest::new(
        fixture.package.clone(),
        ExpectedPolicyRevision::Absent {},
        rev(1),
        request.expected_host_policy.clone(),
    );
    store.apply_selection(fixture.admin, &id("select"), &select, Timestamp(5)).await.unwrap();
    let mut connection = fixture.connection().await;
    sqlx::query("DELETE FROM accounts WHERE id=?").bind(fixture.admin.0).execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(?,'user','replacement',1,1)")
        .bind(fixture.admin.0)
        .execute(&mut connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_logins(account_id,email,password_hash,is_admin) VALUES(?,'replacement@example.com','unused',1)")
        .bind(fixture.admin.0)
        .execute(&mut connection)
        .await
        .unwrap();
    assert_eq!(store.operation(fixture.admin, &id("review")).await.unwrap().unwrap().actor, None);
    conflict(store.apply_review(fixture.admin, &id("review"), &request, Timestamp(6)).await);
    validate_snapshot(&mut connection).await.unwrap();
    assert_eq!(store.gate(fixture.admin).await.unwrap().state, PolicyGateState::Open);
    fixture.db.close().await;
}

#[tokio::test]
async fn terminal_selection_revision_seals_without_increment() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    store.apply_review(fixture.admin, &id("review"), &request, Timestamp(4)).await.unwrap();
    let mut select = PackageSelectionRequest::new(
        fixture.package.clone(),
        ExpectedPolicyRevision::Absent {},
        rev(1),
        request.expected_host_policy.clone(),
    );
    store.apply_selection(fixture.admin, &id("select"), &select, Timestamp(5)).await.unwrap();
    let mut connection = fixture.connection().await;
    // Exhausted counter is an explicit trusted fixture; producing this many real
    // mutations is impossible within the independent bounded receipt limit.
    sqlx::query("UPDATE plugin_selection_heads SET revision=9223372036854775807").execute(&mut connection).await.unwrap();
    select.expected_selection = ExpectedPolicyRevision::Exact { revision: rev(PolicyRevision::MAX) };
    let result = store.apply_selection(fixture.admin, &id("overflow"), &select, Timestamp(6)).await.unwrap();
    let ApplyOutcome::CapacitySealed { gate } = result else {
        panic!("{result:?}");
    };
    assert_eq!(gate.seal.unwrap().reason, PolicySealReason::RevisionExhausted);
    assert_eq!(
        store.inspect(fixture.admin, fixture.package.plugin_id()).await.unwrap().selection.unwrap().revision,
        rev(PolicyRevision::MAX)
    );
    fixture.db.close().await;
}

#[tokio::test]
async fn failed_receipt_insert_rolls_back_staged_history_and_head_without_sealing() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    let mut connection = fixture.connection().await;
    sqlx::query("CREATE TRIGGER reject_test_receipt BEFORE INSERT ON plugin_policy_receipts BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END").execute(&mut connection).await.unwrap();
    let error = store.apply_review(fixture.admin, &id("fails-after-head"), &request, Timestamp(4)).await.unwrap_err();
    assert!(matches!(error, StorageError::Sqlite(_)));
    assert!(store.inspect_package(fixture.admin, &fixture.package).await.unwrap().review.is_none());
    assert!(store.operation(fixture.admin, &id("fails-after-head")).await.unwrap().is_none());
    assert_eq!(store.gate(fixture.admin).await.unwrap().state, PolicyGateState::Open);
    validate_snapshot(&mut connection).await.unwrap();
    fixture.db.close().await;
}

#[tokio::test]
async fn snapshot_rejects_nul_hidden_oversized_scalar_before_pagination() {
    let fixture = Fixture::new().await;
    let store = fixture.db.plugin_policy();
    let request = fixture.review().await;
    store.apply_review(fixture.admin, &id("review"), &request, Timestamp(4)).await.unwrap();
    let mut connection = fixture.connection().await;
    sqlx::query("DROP TRIGGER plugin_policy_receipts_immutable").execute(&mut connection).await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON").execute(&mut connection).await.unwrap();
    sqlx::query("UPDATE plugin_policy_receipts SET id='good'||char(0)||CAST(zeroblob(131072) AS TEXT)")
        .execute(&mut connection)
        .await
        .unwrap();
    assert!(validate_snapshot(&mut connection).await.is_err());
    sqlx::query("UPDATE plugin_policy_receipts SET id=''").execute(&mut connection).await.unwrap();
    assert!(validate_snapshot(&mut connection).await.is_err());
    fixture.db.close().await;
}

#[path = "plugin_policy_epochs.rs"]
mod epochs;

#[path = "plugin_policy_epoch_upgrade.rs"]
mod epoch_upgrade;
