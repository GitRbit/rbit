//! Package ledger atomicity, replay and restart boundaries without filesystem authority.
use rbit_core::{AccountId, Timestamp};
use rbit_plugin_api::PackageManifest;
use rbit_storage::{
    Database, StorageError,
    sqlite::plugin_installations::{InstallationDescriptor, InstallationOperation, OperationError, OperationId, OperationStage},
};
use serde_json::json;

fn descriptor(digest: char, version: &str) -> InstallationDescriptor {
    let manifest: PackageManifest = serde_json::from_value(json!({
        "schema_version":1,
        "plugin":{"id":"example.package", "version":version, "host_api":"^1", "capabilities":["example.read"]},
        "kind":{"type":"data"},
        "files":[{"path":"data/file", "size":1, "sha256":"a".repeat(64)}],
        "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
    }))
    .unwrap();
    InstallationDescriptor::new(manifest, digest.to_string().repeat(64).parse().unwrap(), 4096).unwrap()
}
fn id(value: &str) -> OperationId {
    OperationId::new(value).unwrap()
}
fn conflict<T: std::fmt::Debug>(result: Result<T, StorageError>) {
    let error = result.unwrap_err();
    assert!(matches!(error, StorageError::Conflict(_)), "{error:?}");
}

#[tokio::test]
async fn restart_preserves_pending_and_unconfirmed_without_installing_or_enabling() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rbit.db");
    let database = Database::open(&path).await.unwrap();
    let store = database.plugin_installations();
    let desc = descriptor('a', "1.0.0+build");
    let op = id("operation-1");
    store.begin(&op, &desc, None, Timestamp(1)).await.unwrap();
    let updated = store.transition(&op, 0, OperationStage::AwaitingDurability, Timestamp(2)).await.unwrap();
    assert_eq!(updated.revision, 1);
    drop(store);
    drop(database);
    let database = Database::open(&path).await.unwrap();
    let store = database.plugin_installations();
    assert_eq!(store.operation(&op).await.unwrap().unwrap(), updated);
    assert_eq!(store.recovery(None).await.unwrap(), [updated]);
    assert!(store.installation(&desc.manifest().plugin.id, &desc.manifest().plugin.version).await.unwrap().is_none());
    assert_eq!(store.objects(None).await.unwrap(), Vec::<InstallationDescriptor>::new());
    assert_eq!(database.plugin_settings().desired_enabled("example.package").await.unwrap(), None);
    conflict(store.complete(&op, 0, Timestamp(3)).await);
    let installed = store.complete(&op, 1, Timestamp(3)).await.unwrap();
    assert_eq!(installed.descriptor, desc);
    assert_eq!(installed.installed_at, Timestamp(3));
    assert_eq!(store.recovery(None).await.unwrap(), Vec::<InstallationOperation>::new());
    let done = store.operation(&op).await.unwrap().unwrap();
    assert_eq!(done.stage, OperationStage::Complete);
    assert_eq!(done.revision, 2);
    assert_eq!(store.complete(&op, 2, Timestamp(4)).await.unwrap(), installed);
    conflict(store.transition(&op, 2, OperationStage::Pending, Timestamp(4)).await);
    assert_eq!(database.plugin_settings().desired_enabled("example.package").await.unwrap(), None);
    drop(store);
    drop(database);
    let reopened = Database::open(&path).await.unwrap();
    let store = reopened.plugin_installations();
    assert_eq!(store.installation(&desc.manifest().plugin.id, &desc.manifest().plugin.version).await.unwrap(), Some(installed));
    assert_eq!(store.operation(&op).await.unwrap().unwrap(), done);
}

#[tokio::test]
async fn idempotency_requires_exact_descriptor_and_actor_and_does_not_reset_progress() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let store = database.plugin_installations();
    let desc = descriptor('a', "1.0.0");
    let op = id("operation-1");
    let original = store.begin(&op, &desc, None, Timestamp(1)).await.unwrap();
    assert_eq!(store.begin(&op, &desc, None, Timestamp(2)).await.unwrap(), original);
    conflict(store.begin(&op, &descriptor('b', "1.0.0"), None, Timestamp(2)).await);
    conflict(store.begin(&op, &desc, Some(AccountId(123)), Timestamp(2)).await);
    let failed = store.transition(&op, 0, OperationStage::Error(OperationError::Interrupted), Timestamp(3)).await.unwrap();
    assert_eq!(store.begin(&op, &desc, None, Timestamp(4)).await.unwrap(), failed);
    conflict(store.complete(&op, 1, Timestamp(4)).await);
    store.transition(&op, 1, OperationStage::Pending, Timestamp(5)).await.unwrap();
    store.complete(&op, 2, Timestamp(6)).await.unwrap();
    let other = id("operation-2");
    store.begin(&other, &desc, None, Timestamp(7)).await.unwrap();
    let same = store.complete(&other, 0, Timestamp(8)).await.unwrap();
    assert_eq!(same.installed_at, Timestamp(6));
    assert_eq!(store.objects(None).await.unwrap(), [desc]);
}

#[tokio::test]
async fn concurrent_version_pins_have_one_winner_and_no_partial_losing_object() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let store = database.plugin_installations();
    let first = id("first");
    let second = id("second");
    store.begin(&first, &descriptor('a', "1.0.0"), None, Timestamp(1)).await.unwrap();
    store.begin(&second, &descriptor('b', "1.0.0"), None, Timestamp(1)).await.unwrap();
    let (a, b) = tokio::join!(store.complete(&first, 0, Timestamp(2)), store.complete(&second, 0, Timestamp(2)));
    assert_ne!(a.is_ok(), b.is_ok());
    let winner = match (a, b) {
        (Ok(installed), other) | (other, Ok(installed)) => {
            conflict(other);
            installed
        }
        other => panic!("unexpected results: {other:?}"),
    };
    assert_eq!(store.objects(None).await.unwrap().as_slice(), std::slice::from_ref(&winner.descriptor));
    let recovery = store.recovery(None).await.unwrap();
    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0].revision, 0);
    assert_eq!(recovery[0].stage, OperationStage::Pending);
    let conflicting = if winner.descriptor.digest().as_str().starts_with('a') { 'b' } else { 'a' };
    conflict(store.begin(&id("third"), &descriptor(conflicting, "1.0.0"), None, Timestamp(3)).await);
}

#[tokio::test]
async fn full_semver_build_metadata_and_revision_fences_are_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let store = database.plugin_installations();
    for (token, version, hash) in [("first", "1.0.0+first", 'a'), ("second", "1.0.0+second", 'b')] {
        let desc = descriptor(hash, version);
        let op = id(token);
        store.begin(&op, &desc, None, Timestamp(1)).await.unwrap();
        assert!(store.transition(&op, 0, OperationStage::Complete, Timestamp(2)).await.is_err());
        let (a, b) = tokio::join!(
            store.transition(&op, 0, OperationStage::AwaitingDurability, Timestamp(2)),
            store.transition(&op, 0, OperationStage::Error(OperationError::Publication), Timestamp(2)),
        );
        assert_ne!(a.is_ok(), b.is_ok());
        let current = store.operation(&op).await.unwrap().unwrap();
        let ready = store.transition(&op, current.revision, OperationStage::Pending, Timestamp(3)).await.unwrap();
        store.complete(&op, ready.revision, Timestamp(4)).await.unwrap();
        assert_eq!(
            store.installation(&desc.manifest().plugin.id, &desc.manifest().plugin.version).await.unwrap().unwrap().descriptor,
            desc
        );
    }
    assert_eq!(store.objects(None).await.unwrap().len(), 2);
    assert!(OperationId::new("../unsafe").is_err());
    assert!(OperationId::new(&"a".repeat(65)).is_err());
    assert!(InstallationDescriptor::new(descriptor('a', "1.0.0").manifest().clone(), "a".repeat(64).parse().unwrap(), 0).is_err());
}

#[tokio::test]
async fn recovery_and_object_inventory_use_bounded_cursor_pages() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let store = database.plugin_installations();
    let desc = descriptor('a', "1.0.0");
    for index in 0..130 {
        store.begin(&id(&format!("operation-{index:03}")), &desc, None, Timestamp(1)).await.unwrap();
    }
    let first = store.recovery(None).await.unwrap();
    assert_eq!(first.len(), 128);
    let second = store.recovery(Some(&first.last().unwrap().id)).await.unwrap();
    assert_eq!(second.len(), 2);
    assert_ne!(first[0].id, second[0].id);
    store.complete(&first[0].id, 0, Timestamp(2)).await.unwrap();
    let objects = store.objects(None).await.unwrap();
    assert_eq!(objects, [desc]);
    assert_eq!(store.objects(Some(objects[0].digest())).await.unwrap(), Vec::<InstallationDescriptor>::new());
}

#[tokio::test]
async fn operation_capacity_fails_closed_without_resetting_existing_work() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rbit.db");
    let database = Database::open(&path).await.unwrap();
    let store = database.plugin_installations();
    let desc = descriptor('a', "1.0.0");
    let original = store.begin(&id("original"), &desc, None, Timestamp(1)).await.unwrap();
    let raw = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&path)).await.unwrap();
    // Fill the real persisted journal in one transaction, rather than spending
    // thousands of synchronous commits to reach the host's retention bound.
    sqlx::query(
        "WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM numbers WHERE n < 4095) \
         INSERT INTO plugin_package_operations \
         SELECT 'filled-' || n, plugin_id, version, digest, manifest, wire_bytes, actor, stage, revision, error, created_at, updated_at \
         FROM numbers CROSS JOIN plugin_package_operations WHERE id = 'original'",
    )
    .execute(&raw)
    .await
    .unwrap();
    assert!(store.begin(&id("overflow"), &desc, None, Timestamp(2)).await.is_err());
    assert!(store.operation(&id("overflow")).await.unwrap().is_none());
    assert_eq!(store.begin(&id("original"), &desc, None, Timestamp(2)).await.unwrap(), original);
    assert_eq!(store.objects(None).await.unwrap(), Vec::<InstallationDescriptor>::new());
    assert_eq!(store.recovery(None).await.unwrap().len(), 128);
    raw.close().await;
}
