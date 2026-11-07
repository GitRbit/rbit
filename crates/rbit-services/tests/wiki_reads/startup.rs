//! Actual native cursor→owned FULL scratch composition, not publication fixtures.
use super::{
    acceptance::{capacity, command},
    fixture::Fixture,
};
use rbit_core::Timestamp;
use rbit_git::wikis::{WikiObjectWriter, WikiObjects};
use rbit_services::wikis::startup::{WikiStartupCatalog, WikiStartupScanError};
use rbit_storage::sqlite::wiki_mutations::full::FullWikiWriter;
use rbit_wikis::{
    ownership::WikiCatalogOwnership,
    startup::{
        WikiStartupBatchSize, WikiStartupScope,
        objects::{MAX_SCAN_OBJECT_BYTES, WikiScanBudget},
        scratch::WikiScratchLimits,
    },
};

fn metadata(fixture: &Fixture, pending: bool) -> WikiStartupScope {
    WikiStartupScope { wiki: fixture.wiki, repository: fixture.repo.repo.repo.id, enabled: true, head: None, pending, orphaned: false }
}
async fn catalog(fixture: &Fixture, prepare: bool, corrupt: bool) -> WikiStartupCatalog {
    let full = FullWikiWriter::open(fixture.database.wiki_catalog_lease().await.unwrap()).await.unwrap();
    let root = fixture.directory.path().join("startup-objects");
    let actual = full.catalog();
    let objects = WikiObjects::open(&root, actual.store_id(), fixture.work.clone()).unwrap();
    let objects = WikiObjectWriter::open(objects).unwrap().bind_catalog(actual).unwrap();
    if prepare {
        let intent = fixture
            .database
            .wiki_mutations()
            .accept(command(fixture, 1, "exact accepted private source"), Timestamp(7), capacity())
            .await
            .unwrap();
        let prepared = objects.prepare(intent).await.unwrap();
        drop(prepared);
    }
    if corrupt {
        let directory = root.join(format!("{}/{}.git/objects", fixture.repo.repo.repo.id.0, fixture.wiki.get()));
        let path = std::fs::read_dir(directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.len() == 2))
            .find_map(|entry| std::fs::read_dir(entry.path()).unwrap().next().map(|entry| entry.unwrap().path()))
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        #[cfg(windows)]
        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "Windows attributes are separate from Unix permissions, which use explicit 0600 above"
        )]
        {
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        std::fs::write(path, b"not a complete zlib object").unwrap();
    }
    let recovery = full.begin_recovery(objects).await.unwrap();
    assert!(recovery.cleanup_scratch(WikiStartupBatchSize::new(1).unwrap()).await.unwrap());
    WikiStartupCatalog::from_recovery(recovery, WikiScratchLimits::new(1, 10, 10, 4096).unwrap()).await.unwrap()
}

#[tokio::test]
async fn real_prepared_objects_enter_scratch_once_then_only_advance_to_sql_agreement() {
    let fixture = Fixture::new().await;
    let catalog = catalog(&fixture, true, false).await;
    let mut scope = catalog.scope(metadata(&fixture, true)).await.unwrap();
    let mut hashed = 0;
    let mut records = 0;
    loop {
        let progress = scope.advance(WikiScanBudget::new(1, MAX_SCAN_OBJECT_BYTES).unwrap()).await.unwrap();
        hashed += u64::from(progress.objects_hashed);
        records += u64::from(progress.records_applied);
        if progress.scope_ended {
            let usage = progress.usage.unwrap();
            assert_eq!((usage.scopes, usage.objects, usage.edges), (1, 3, 2));
            break;
        }
    }
    assert_eq!((hashed, records), (3, 9));
    assert!(matches!(scope.advance(WikiScanBudget::new(1, MAX_SCAN_OBJECT_BYTES).unwrap()).await, Err(WikiStartupScanError::Ended)));
    let mut cursor = None;
    let mut checked = 0;
    loop {
        let batch = catalog.validate_edges(cursor, WikiStartupBatchSize::new(1).unwrap()).await.unwrap();
        checked += batch.checked();
        cursor = batch.into_next();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(checked, 2);
    let current: (String, i64) =
        sqlx::query_as("SELECT s.phase,(SELECT count(*) FROM wiki_pages) FROM wiki_scan_control s WHERE singleton=1")
            .fetch_one(&fixture.raw)
            .await
            .unwrap();
    assert_eq!(current, ("agreement".to_owned(), 0));
    catalog.close().await.unwrap();
}

#[tokio::test]
async fn empty_catalog_uses_real_absence_without_initializing_a_git_scope() {
    let fixture = Fixture::new().await;
    let catalog = catalog(&fixture, false, false).await;
    let mut scope = catalog.scope(metadata(&fixture, false)).await.unwrap();
    let progress = scope.advance(WikiScanBudget::new(1, MAX_SCAN_OBJECT_BYTES).unwrap()).await.unwrap();
    assert!(progress.scope_ended);
    assert_eq!((progress.objects_hashed, progress.records_applied), (0, 0));
    assert!(!fixture.directory.path().join("startup-objects").join(fixture.repo.repo.repo.id.0.to_string()).exists());
    catalog.close().await.unwrap();
}

#[tokio::test]
async fn corrupt_native_input_or_rejected_work_poison_continuation_without_sql_eof_or_visibility() {
    for corrupt in [false, true] {
        let fixture = Fixture::new().await;
        let catalog = catalog(&fixture, true, corrupt).await;
        let mut scope = catalog.scope(metadata(&fixture, true)).await.unwrap();
        let held = if corrupt { None } else { Some(fixture.work.try_reserve().unwrap()) };
        assert!(scope.advance(WikiScanBudget::new(256, 16 * MAX_SCAN_OBJECT_BYTES).unwrap()).await.is_err());
        drop(held);
        assert!(matches!(scope.advance(WikiScanBudget::new(1, MAX_SCAN_OBJECT_BYTES).unwrap()).await, Err(WikiStartupScanError::Poisoned)));
        let rows: (i64, i64) = sqlx::query_as("SELECT scanned,(SELECT count(*) FROM wiki_pages) FROM wiki_scan_scopes LIMIT 1")
            .fetch_one(&fixture.raw)
            .await
            .unwrap();
        assert_eq!(rows, (0, 0));
        catalog.close().await.unwrap();
    }
}
