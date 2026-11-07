//! Real temporary SQL; input records are data fixtures, not native integrity proof.
use super::*;
use crate::sqlite::wiki_mutations::full::{FullWikiWriter, fencing_tests, tests as fixture};
use rbit_core::{AccountId, Timestamp};
use rbit_wikis::{
    WikiPageId, WikiRevisionId,
    startup::objects::{WikiScanRecord as R, WikiScannedKind as T},
};

fn limits(scopes: u32, objects: u32, edges: u32, bytes: u64) -> WikiScratchLimits {
    WikiScratchLimits::new(scopes, objects, edges, bytes).unwrap()
}
fn id(byte: u8) -> WikiRevisionId {
    WikiRevisionId::try_from([byte; 20].as_slice()).unwrap()
}
fn sample() -> Vec<R> {
    vec![
        R::Object { id: id(2), kind: T::Tree, bytes: 32 },
        R::Edge { source: id(2), ordinal: 0, target: id(1), kind: T::Blob, page: Some(WikiPageId::new(1).unwrap()) },
        R::Complete { id: id(2) },
        R::Object { id: id(1), kind: T::Blob, bytes: 4 },
        R::Complete { id: id(1) },
        R::Object { id: id(3), kind: T::Commit, bytes: 100 },
        R::Commit { id: id(3), actor: AccountId(1), incarnation: Some(1), at: Timestamp(7) },
        R::Edge { source: id(3), ordinal: 0, target: id(2), kind: T::Tree, page: None },
        R::Complete { id: id(3) },
    ]
}
async fn counts(database: &crate::Database) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM wiki_scan_edges),(SELECT count(*) FROM wiki_scan_objects),(SELECT count(*) FROM wiki_scan_scopes)",
    )
    .fetch_one(&database.pool)
    .await
    .unwrap()
}
async fn setup(policy: WikiScratchLimits) -> (tempfile::TempDir, crate::Database, WikiScratchLedger<fencing_tests::FileObjects>) {
    let (directory, database, writer, _command) = fixture::fixture().await;
    let recovery = fencing_tests::fenced(writer, directory.path()).await;
    assert!(recovery.cleanup_scratch(WikiStartupBatchSize::new(1).unwrap()).await.unwrap());
    let ledger = recovery.begin_scratch(policy).await.unwrap();
    (directory, database, ledger)
}

#[tokio::test]
async fn ordered_batches_persist_unique_objects_then_check_edges_by_bounded_keyset_without_readiness() {
    let (_directory, database, ledger) = setup(limits(1, 3, 2, 704)).await;
    let scope = ledger.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
    let mut usage = None;
    for records in sample().chunks(2) {
        usage = Some(scope.record(records.to_vec()).await.unwrap());
    }
    assert_eq!(usage.unwrap(), WikiScratchUsage { scopes: 1, objects: 3, edges: 2, logical_bytes: 704 });
    assert!(matches!(ledger.validate_edges(None, WikiStartupBatchSize::new(1).unwrap()).await, Err(WikiWriteError::ScratchIncomplete)));
    scope.finish().await.unwrap();
    let first = ledger.validate_edges(None, WikiStartupBatchSize::new(1).unwrap()).await.unwrap();
    assert_eq!(first.checked(), 1);
    let next = first.into_next();
    assert!(next.is_some());
    let second = ledger.validate_edges(next, WikiStartupBatchSize::new(1).unwrap()).await.unwrap();
    assert_eq!(second.checked(), 1);
    assert!(second.into_next().is_none());
    let phase: String =
        sqlx::query_scalar("SELECT phase FROM wiki_scan_control WHERE singleton=1").fetch_one(&database.pool).await.unwrap();
    assert_eq!(phase, "agreement", "edge agreement is not native/SQL/root readiness");
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_pages").fetch_one(&database.pool).await.unwrap();
    assert_eq!(visible, 0);
    ledger.close().await.unwrap();
}

#[tokio::test]
async fn capacity_failure_and_duplicate_object_rollback_the_entire_batch_including_charges() {
    let (_directory, database, ledger) = setup(limits(1, 1, 1, 192)).await;
    let scope = ledger.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
    assert!(matches!(
        scope.record(vec![R::Object { id: id(1), kind: T::Blob, bytes: 1 }, R::Object { id: id(2), kind: T::Blob, bytes: 1 }]).await,
        Err(WikiWriteError::ScratchCapacity)
    ));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM wiki_scan_objects").fetch_one(&database.pool).await.unwrap();
    assert_eq!(rows, 0);
    let usage = scope.record(vec![R::Object { id: id(1), kind: T::Blob, bytes: 1 }, R::Complete { id: id(1) }]).await.unwrap();
    assert_eq!((usage.objects, usage.logical_bytes), (1, 192));
    assert!(scope.record(vec![R::Object { id: id(1), kind: T::Blob, bytes: 1 }]).await.is_err());
    let counts: (i64, i64) =
        sqlx::query_as("SELECT objects,logical_bytes FROM wiki_scan_control WHERE singleton=1").fetch_one(&database.pool).await.unwrap();
    assert_eq!(counts, (1, 192));
    scope.finish().await.unwrap();
    ledger.close().await.unwrap();
}

#[tokio::test]
async fn missing_or_wrong_kind_edges_and_incomplete_commit_cannot_advance_to_agreement() {
    for wrong_kind in [false, true] {
        let (_directory, database, ledger) = setup(limits(1, 3, 3, 2048)).await;
        let scope = ledger.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
        let mut records = vec![
            R::Object { id: id(2), kind: T::Tree, bytes: 32 },
            R::Edge { source: id(2), ordinal: 0, target: id(1), kind: T::Blob, page: Some(WikiPageId::new(1).unwrap()) },
            R::Complete { id: id(2) },
        ];
        if wrong_kind {
            records.extend([R::Object { id: id(1), kind: T::Tree, bytes: 0 }, R::Complete { id: id(1) }]);
        }
        scope.record(records).await.unwrap();
        scope.finish().await.unwrap();
        assert!(matches!(ledger.validate_edges(None, WikiStartupBatchSize::new(1).unwrap()).await, Err(WikiWriteError::ScratchIncomplete)));
        let phase: String =
            sqlx::query_scalar("SELECT phase FROM wiki_scan_control WHERE singleton=1").fetch_one(&database.pool).await.unwrap();
        assert_eq!(phase, "scan", "failed edge transaction rolls back phase change");
        ledger.close().await.unwrap();
    }
    let (_directory, _database, ledger) = setup(limits(1, 3, 3, 2048)).await;
    let scope = ledger.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
    scope.record(vec![R::Object { id: id(3), kind: T::Commit, bytes: 100 }]).await.unwrap();
    assert!(scope.record(vec![R::Complete { id: id(3) }]).await.is_err());
    assert!(matches!(scope.finish().await, Err(WikiWriteError::ScratchIncomplete)));
    ledger.close().await.unwrap();
}

#[tokio::test]
async fn ordered_edge_ordinal_and_page_uniqueness_are_enforced_before_completion() {
    let (_directory, _database, ledger) = setup(limits(1, 1, 3, 1024)).await;
    let scope = ledger.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
    scope.record(vec![R::Object { id: id(2), kind: T::Tree, bytes: 64 }]).await.unwrap();
    let edge = |ordinal| R::Edge { source: id(2), ordinal, target: id(1), kind: T::Blob, page: Some(WikiPageId::new(1).unwrap()) };
    assert!(scope.record(vec![edge(1)]).await.is_err());
    scope.record(vec![edge(0)]).await.unwrap();
    assert!(scope.record(vec![edge(1)]).await.is_err(), "same page may not be duplicated under another ordinal");
    scope.record(vec![R::Complete { id: id(2) }]).await.unwrap();
    scope.finish().await.unwrap();
    ledger.close().await.unwrap();
}

#[tokio::test]
async fn old_rows_require_bounded_cleanup_and_never_become_new_owner_proof() {
    let (directory, database, ledger) = setup(limits(1, 3, 2, 2048)).await;
    let scope = ledger.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
    scope.record(sample()).await.unwrap();
    scope.finish().await.unwrap();
    drop(scope);
    ledger.close().await.unwrap();
    drop(ledger);
    let second = directory.path().join("second");
    std::fs::create_dir(&second).unwrap();
    let writer = FullWikiWriter::open(database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let recovery = fencing_tests::fenced(writer, &second).await;
    let closing = recovery.writer.clone();
    assert!(matches!(recovery.begin_scratch(limits(1, 3, 2, 2048)).await, Err(WikiWriteError::ScratchCleanupRequired)));
    closing.close().await.unwrap();
    drop(closing);
    let third = directory.path().join("third");
    std::fs::create_dir(&third).unwrap();
    let writer = FullWikiWriter::open(database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let recovery = fencing_tests::fenced(writer, &third).await;
    let size = WikiStartupBatchSize::new(1).unwrap();
    loop {
        let before = counts(&database).await;
        let done = recovery.cleanup_scratch(size).await.unwrap();
        let after = counts(&database).await;
        assert!(before.0 - after.0 <= 1 && before.1 - after.1 <= 1 && before.2 - after.2 <= 1);
        if done {
            assert_eq!(after, (0, 0, 0));
            break;
        }
        assert_ne!(before, after, "cleanup must progress without unbounded eligibility scans");
    }
    let fresh = recovery.begin_scratch(limits(1, 3, 2, 2048)).await.unwrap();
    fresh.close().await.unwrap();
}

#[tokio::test]
async fn edge_and_logical_byte_limits_rollback_without_advancing_the_source_ordinal() {
    for policy in [limits(1, 1, 1, 1024), limits(1, 1, 3, 320)] {
        let (_directory, database, ledger) = setup(policy).await;
        let scope = ledger.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
        scope
            .record(vec![
                R::Object { id: id(2), kind: T::Tree, bytes: 64 },
                R::Edge { source: id(2), ordinal: 0, target: id(1), kind: T::Blob, page: Some(WikiPageId::new(1).unwrap()) },
            ])
            .await
            .unwrap();
        let result = scope
            .record(vec![R::Edge { source: id(2), ordinal: 1, target: id(1), kind: T::Blob, page: Some(WikiPageId::new(2).unwrap()) }])
            .await;
        assert!(matches!(result, Err(WikiWriteError::ScratchCapacity)));
        let values:(i64,i64,i64)=sqlx::query_as("SELECT c.edges,c.logical_bytes,o.next_ordinal FROM wiki_scan_control c JOIN wiki_scan_objects o ON o.epoch=c.epoch WHERE c.singleton=1")
            .fetch_one(&database.pool).await.unwrap();
        assert_eq!(values, (1, 320, 1));
        ledger.close().await.unwrap();
    }
}

#[tokio::test]
async fn foreign_cursor_stale_epoch_and_default_writers_cannot_advance_scratch() {
    let (_one_directory, one, first) = setup(limits(1, 3, 2, 2048)).await;
    let (_two_directory, _two, second) = setup(limits(1, 3, 2, 2048)).await;
    let scope = first.scope(WikiId::new(1).unwrap(), RepoId(1)).await.unwrap();
    scope.record(sample()).await.unwrap();
    scope.finish().await.unwrap();
    let cursor = first.validate_edges(None, WikiStartupBatchSize::new(1).unwrap()).await.unwrap().into_next();
    assert!(cursor.is_some());
    assert!(matches!(second.validate_edges(cursor, WikiStartupBatchSize::new(1).unwrap()).await, Err(WikiWriteError::Binding)));
    assert!(matches!(first.scope(WikiId::new(1).unwrap(), RepoId(99)).await, Err(WikiWriteError::Binding)));
    let denied = sqlx::query("UPDATE wiki_scan_control SET phase='agreement' WHERE singleton=1").execute(&one.pool).await.unwrap_err();
    assert_eq!(denied.as_database_error().unwrap().message(), "wiki scratch writer refused");
    sqlx::query("UPDATE wiki_writer_protocol SET epoch=epoch+1").execute(&first.owner.recovery.writer.0.pool).await.unwrap();
    assert!(matches!(first.validate_edges(None, WikiStartupBatchSize::new(1).unwrap()).await, Err(WikiWriteError::Binding)));
    first.close().await.unwrap();
    second.close().await.unwrap();
}
